import logging
import os
import traceback
import uuid
from contextlib import asynccontextmanager
from typing import List, Optional
import threading
import anyio
from kubernetes import client, config
from llm_sandbox import SandboxSession
from llm_sandbox.pool import ExhaustionStrategy, PoolConfig, create_pool_manager
from mcp.server.fastmcp import FastMCP
from mcp.server.transport_security import TransportSecuritySettings
from starlette.applications import Starlette
from starlette.requests import Request
from starlette.responses import JSONResponse
from starlette.routing import Mount
import time
from llm_sandbox.kubernetes import SandboxKubernetesSession

# Патчим генерацию имени пода для TypeScript (sandbox-typescript-<uuid>)
_original_init = SandboxKubernetesSession.__init__

def _patched_init(self, *args, **kwargs):
    _original_init(self, *args, **kwargs)

    if not self.using_existing_container:
        short_uuid = uuid.uuid4().hex[:8]
        self.pod_name = f"sandbox-{SANDBOX_LANG.lower()}-{short_uuid}"
        self.pod_manifest.setdefault("metadata", {})["name"] = self.pod_name

SandboxKubernetesSession.__init__ = _patched_init

logging.basicConfig(
    level=os.environ.get("LOG_LEVEL", "INFO"),
    format="%(asctime)s %(levelname)s %(name)s %(message)s",
)
logger = logging.getLogger("llm-sandbox-adapter")

security_settings = TransportSecuritySettings(
    enable_dns_rebinding_protection=False
)

mcp = FastMCP(
    "llm-sandbox-adapter",
    transport_security=security_settings,
)

BACKEND = os.environ.get("BACKEND", "kubernetes")
NAMESPACE = os.environ.get("NAMESPACE", "default")

SANDBOX_IMAGE = os.environ.get(
    "SANDBOX_IMAGE",
    "harbor.neoflex.ru/test-images/llm-sandbox-base:0.0.3",
)
SANDBOX_LANG = os.environ.get("SANDBOX_LANG", "python")
SKIP_ENV_SETUP = os.environ.get("SKIP_ENV_SETUP", "true").lower() == "true"
POD_CPU_REQUEST = os.environ.get("POD_CPU_REQUEST", "250m")
POD_CPU_LIMIT = os.environ.get("POD_CPU_LIMIT", "500m")
POD_MEM_REQUEST = os.environ.get("POD_MEM_REQUEST", "256Mi")
POD_MEM_LIMIT = os.environ.get("POD_MEM_LIMIT", "512Mi")
POD_ACTIVE_DEADLINE_SECONDS = int(os.environ.get("POD_ACTIVE_DEADLINE_SECONDS", "7200"))

# TypeScript не входит в число нативно поддерживаемых llm_sandbox языков. Поэтому пул и сессии для TypeScript открываются как "javascript" (нативно поддерживается), а сама компиляция/запуск .ts делается вручную через execute_command().
EFFECTIVE_LANG = "javascript" if SANDBOX_LANG == "typescript" else SANDBOX_LANG

def build_pod_manifest() -> dict:
    metadata = {
        "labels": {
            "app.kubernetes.io/name": "llm-sandbox",
            "llm-sandbox/declared-lang": SANDBOX_LANG,
        },
        "annotations": {
            "sidecar.istio.io/inject": "false",
        },
    }
    if _SELF_POD_UID:
        metadata["ownerReferences"] = [{
            "apiVersion": "v1",
            "kind": "Pod",
            "name": _SELF_POD_NAME,
            "uid": _SELF_POD_UID,
            "controller": True,
            "blockOwnerDeletion": False,
        }]
    return {
        "apiVersion": "v1",
        "kind": "Pod",
        "metadata": metadata,
        "spec": {
            "automountServiceAccountToken": False,  # под с недоверенным кодом не должен получать токен ServiceAccount и иметь доступ к k8s API изнутри
            "activeDeadlineSeconds": POD_ACTIVE_DEADLINE_SECONDS,  # жёсткий потолок жизни пода на случай, если cleanup не отработает
            "securityContext": {
                "runAsNonRoot": True,
                "runAsUser": 64535,
                "runAsGroup": 64535,
                "seccompProfile": {"type": "RuntimeDefault"},
            },
            "containers": [{
                "name": "sandbox-container",
                "image": SANDBOX_IMAGE,
                "command": ["sleep", "infinity"],
                "tty": True,
                "securityContext": {
                    "allowPrivilegeEscalation": False,
                    "capabilities": {"drop": ["ALL"]},
                    "runAsNonRoot": True,
                    "runAsUser": 64535,
                    "seccompProfile": {"type": "RuntimeDefault"},
                    "readOnlyRootFilesystem": False,
                },
                "resources": {
                    "requests": {"cpu": POD_CPU_REQUEST, "memory": POD_MEM_REQUEST},
                    "limits": {"cpu": POD_CPU_LIMIT, "memory": POD_MEM_LIMIT},
                },
            }],
            "restartPolicy": "Never",
        },
    }


class ThreadLocalCoreV1Api:
    #Прокси вокруг kubernetes.client.CoreV1Api с отдельным экземпляром клиента на каждый поток (health-check, pre-warming, каждый воркер-поток anyio.to_thread,выполняющий exec кода). Отдаёт настоящие bound-методы реального CoreV1Api текущего потока напрямую (через getattr), без собственной обёртки-замыкания — это важно, т.к. kubernetes.stream.stream() внутри llm_sandbox читает func.__self__.api_client, и обёртка-замыкание это бы сломала.

    def __init__(self) -> None:
        self._local = threading.local()

    def _get(self) -> "client.CoreV1Api":
        if not hasattr(self._local, "api"):
            self._local.api = client.CoreV1Api()
            logger.info(
                "Created dedicated CoreV1Api for thread %s",
                threading.current_thread().name,
            )
        return self._local.api

    def __getattr__(self, name):
        return getattr(self._get(), name)


try:
    config.load_incluster_config()
    k8s_client = ThreadLocalCoreV1Api()
    logger.info("Kubernetes client initialized with in-cluster config")
except Exception:
    logger.exception("Failed to initialize Kubernetes client")
    raise

def _wait_for_k8s_api_ready(max_attempts: int = 15, delay_seconds: float = 2.0) -> None:
    probe_api = client.CoreV1Api()
    for attempt in range(1, max_attempts + 1):
        try:
            probe_api.get_api_resources()
            logger.info("Kubernetes API is reachable (attempt %d/%d)", attempt, max_attempts)
            return
        except Exception as exc:
            logger.warning("Kubernetes API not yet reachable (attempt %d/%d): %s", attempt, max_attempts, exc)
            time.sleep(delay_seconds)
    logger.error("Kubernetes API still unreachable after %d attempts — proceeding anyway", max_attempts)

# Определяем под адаптера — чтобы привязать к нему песочницы через ownerReferences. HOSTNAME внутри пода Kubernetes по умолчанию равен имени пода.
_SELF_POD_NAME = os.environ.get("HOSTNAME", "")
try:
    _self_pod = client.CoreV1Api().read_namespaced_pod(name=_SELF_POD_NAME, namespace=NAMESPACE)
    _SELF_POD_UID = _self_pod.metadata.uid
    logger.info("Resolved own pod for ownerReferences: name=%s uid=%s", _SELF_POD_NAME, _SELF_POD_UID)
except Exception:
    logger.exception(
        "Failed to resolve own pod (%s) for ownerReferences — sandbox pods will NOT be auto-cleaned by Kubernetes GC",
        _SELF_POD_NAME,
    )
    _SELF_POD_UID = None

# Пул подов
POOL_MIN_SIZE = int(os.environ.get("POOL_MIN_SIZE", "1"))
POOL_MAX_SIZE = int(os.environ.get("POOL_MAX_SIZE", "3"))

pool_manager = create_pool_manager(
    backend=BACKEND,
    config=PoolConfig(
        min_pool_size=POOL_MIN_SIZE,
        max_pool_size=POOL_MAX_SIZE,
        enable_prewarming=True,
        exhaustion_strategy=ExhaustionStrategy.WAIT,
        acquisition_timeout=30.0,
        idle_timeout=300.0,
        max_container_lifetime=3600.0,  # периодическая замена подов пула, чтобы не копилось состояние/дрифт
    ),
    lang=EFFECTIVE_LANG,
    client=k8s_client,
    pod_manifest=build_pod_manifest(),
    namespace=NAMESPACE,
)


def _run_code_sync(code: str) -> str:
    session = SandboxSession(
        lang=EFFECTIVE_LANG,
        workdir="/tmp",
        pool=pool_manager,
        verbose=False,
    )
    try:
        session.open()

        if SANDBOX_LANG == "typescript":
            # Kubernetes-бэкенд не умеет диспетчеризовать язык "typescript" сам (см. комментарий у EFFECTIVE_LANG) — компилируем и запускаем вручную через execute_command(), минуя языковый run().
            write_result = session.execute_command(
                f"mkdir -p /tmp/ts-src && cat > /tmp/ts-src/main.ts << 'TSEOF'\n{code}\nTSEOF"
            )
            if write_result.exit_code != 0:
                return f"STDOUT:\n\nSTDERR:\nFailed to write source file:\n{write_result.stderr}"

            compile_result = session.execute_command(
                "npx tsc /tmp/ts-src/main.ts --outDir /tmp/ts-build --target es2020 --module commonjs"
            )
            if compile_result.exit_code != 0:
                return f"STDOUT:\n\nSTDERR:\n{compile_result.stderr}"

            result = session.execute_command("node /tmp/ts-build/main.js")
        else:
            result = session.run(code)

        return f"STDOUT:\n{result.stdout}\n\nSTDERR:\n{result.stderr}"
    finally:
        session.close()


@mcp.tool()
async def execute_code(code: str, libraries: Optional[List[str]] = None) -> str:
    if libraries and SKIP_ENV_SETUP:
        return (
            "ERROR: on-the-fly установка библиотек отключена в этом окружении "
            "(SKIP_ENV_SETUP=true). Нужные пакеты должны быть предустановлены в образ "
            f"песочницы ({SANDBOX_IMAGE})."
        )

    error_id = uuid.uuid4().hex[:8]
    try:
        return await anyio.to_thread.run_sync(_run_code_sync, code)
    except Exception:
        logger.exception("execute_code failed [error_id=%s]", error_id)
        return f"ERROR: code execution failed (id={error_id}). See server logs for details."


@mcp.custom_route("/health", methods=["GET"])
async def health_check(request: Request):
    stats = pool_manager.get_stats()
    return JSONResponse({
        "status": "ok",
        "backend": BACKEND,
        "namespace": NAMESPACE,
        "sandbox_lang": SANDBOX_LANG,
        "pool": stats,
    })


@asynccontextmanager
async def starlette_lifespan(app):
    logger.info("Starting up...")
    async with mcp.session_manager.run():
        yield
    logger.info("Shutting down...")
    pool_manager.close()


app = Starlette(
    routes=[Mount('/', app=mcp.streamable_http_app())],
    lifespan=starlette_lifespan,
)