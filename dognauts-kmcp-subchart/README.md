# dognauts-kmcp-subchart

Сабчарт для развёртывания MCP-серверов через KMCP (`kagent.dev/v1alpha1 MCPServer`). Поддерживает режим установки **nested** (`projectAnchor.isNestedRelease: true`) — внутрь уже существующего проекта 2.0, патчит `spec.links` чужого анкора.
Режим **standalone** (`projectAnchor.isNestedRelease: false`) — не поддерживается.
Многократная установка (keyword `supportsMultipleInstalls`) - не поддерживается (данный nested-чарт можно установить в проект только один раз).

## Что делает

1. **В nested-режиме** не создаёт `ProjectAnchor` (namespace, casbin-owner и базовые netpolicy уже принадлежат родительскому проекту), а только патчит `spec.links` его анкора.
2. Добавляет/обновляет в `ProjectAnchor.spec.links` запись `kmcp-<releaseName>` типа `componentContext`.
3. При удалении удаляет только свою запись, не трогая остальные `links`.
4. Разворачивает собственную тройку сервисов — `dognauts-resource-handler` / `dognauts-helm-service` / `dognauts-template-engine-service` под алиасами `kmcp-resource-handler` / `kmcp-helm-service` / `kmcp-template-service` — со своим RBAC и `x-service-name` для casbin, чтобы права на управление MCP-серверами были отделены от прав на управление kagent-агентами.
5. Кладёт служебные секреты (git-токены, helm-репозиторий) в Vault перед установкой/обновлением и удаляет их перед снятием релиза, с отдельным ServiceAccount (`vault-job-sa.yaml`) и NetworkPolicy (`vault-jobs-netpolicy.yaml`).

# LLM Sandbox MCP Adapter

MCP-сервер (Model Context Protocol), который выполняет недоверенный пользовательский код в изолированных Kubernetes-подах. Построен на базе [`llm-sandbox`](https://github.com/vndee/llm-sandbox) и [FastMCP](https://github.com/jlowin/fastmcp).

Сервер предоставляет один MCP-инструмент `execute_code` и отдаёт его по Streamable HTTP. Каждое выполнение кода запускается в короткоживущем, сетевом изолированном поде-песочнице.

## MCP-адаптер (MCP-server)

### Как это работает

1. Клиент вызывает MCP-инструмент `execute_code(code)` по HTTP.
2. Адаптер берёт прогретый под-песочницу из пула.
3. Код выполняется через потоковый `exec` в под (через Kubernetes API — сам адаптер никогда не общается с подом-песочницей по сети).
4. Результат (`STDOUT` / `STDERR`) возвращается клиенту.
5. Поды-песочницы чистятся автоматически:
   - через `ownerReferences` на поде-адаптере (GC Kubernetes),
   - через `activeDeadlineSeconds` (жёсткий потолок жизни),
   - через `max_container_lifetime` в конфиге пула (периодическая ротация).

### Авторизация

Каждый запрос (кроме `/health`) должен нести заголовок: Authorization: Bearer <MCP_API_KEY>
Эндпоинт `/health` всегда открыт (используется пробами Kubernetes).

### Поддерживаемые языки (имеющиеся образы)

| `SANDBOX_LANG` | Примечания |
|---|---|
| `python` | Нативно поддерживается `llm-sandbox` |
| `javascript` | Нативно поддерживается `llm-sandbox` |
| `typescript` | Обрабатывается специально, см. ниже |
| `go` | Нативно поддерживается `llm-sandbox` |

Полный список поддерживаемых языков можно найти здесь: https://github.com/vndee/llm-sandbox

### TypeScript

TypeScript не является нативно поддерживаемым языком, добавлен в исходный код отдельно: Kubernetes-бэкенд `llm-sandbox` проверяет аргумент `lang` через enum `SupportedLanguage` при **каждом** создании сессии (включая внутренний прогрев пула и health-check). Языка `typescript` в этом enum нет, поэтому пул и сессии внутри открываются как `javascript` (`EFFECTIVE_LANG`), а компиляция и запуск `.ts` делаются вручную через `execute_command()`.
Как побочный эффект, библиотека именует свои поды `sandbox-javascript-<uuid>`.
Патч в `mcp_adapter.py`, опирающийся на внутренний API `llm-sandbox`, переписывает имя пода на `sandbox-typescript-<uuid>` и добавляет метку `llm-sandbox/declared-lang: <SANDBOX_LANG>`, чтобы:
- реальный язык был виден в `kubectl get pods -L llm-sandbox/declared-lang`;
- Cilium-политики могли надёжно селектить поды-песочницы.

### Изоляция подов-песочниц

Поды-песочницы намеренно минимальны:

- `automountServiceAccountToken: False` — из песочницы нет доступа к Kubernetes API;
- `runAsNonRoot`, `runAsUser: 64535`, `runAsGroup: 64535`, `seccompProfile: RuntimeDefault`;
- `capabilities.drop: ["ALL"]`, `allowPrivilegeEscalation: false`;
- метка `app.kubernetes.io/name: llm-sandbox` — используется как селектор для Cilium `NetworkPolicy`, которая запрещает весь ingress и egress (см. блок `networkPolicy.policies.llm-sandbox` в родительском чарте);
- метка `llm-sandbox/declared-lang: <lang>` — для человекочитаемой идентификации;
- аннотация `sidecar.istio.io/inject: "false"` — без сайдкара service-mesh.

Сам адаптер никогда не обращается к поду-песочнице по TCP — вся коммуникация идёт через Kubernetes API (`exec`), поэтому песочнице сеть не нужна вообще.
Функция загрузки библиотек в под-песочницу в рантайме, хоть и поддерживается в нативном llm-sandbox, в данной реализации намеренно отключена, в образах подов-песочниц используются предзагруженные библиотеки.

### Переменные окружения

| Переменная | По умолчанию | Описание |
|---|---|---|
| `BACKEND` | `kubernetes` | Бэкенд `llm-sandbox`. |
| `NAMESPACE` | `default` | Namespace, где создаются поды-песочницы. |
| `SANDBOX_IMAGE` | `harbor.neoflex.ru/test-images/llm-sandbox-base:0.0.3` | Образ песочницы. |
| `SANDBOX_LANG` | `python` | Используемый язык для пода-песочницы: `python`, `go`, `javascript`, `typescript`. |
| `SKIP_ENV_SETUP` | `true` | Если `true`, установка библиотек в рантайме отклоняется. |
| `POD_CPU_REQUEST` | `250m` | CPU request пода-песочницы. |
| `POD_CPU_LIMIT` | `500m` | CPU limit пода-песочницы. |
| `POD_MEM_REQUEST` | `256Mi` | Memory request пода-песочницы. |
| `POD_MEM_LIMIT` | `512Mi` | Memory limit пода-песочницы. |
| `POD_ACTIVE_DEADLINE_SECONDS` | `7200` | Жёсткий потолок жизни пода-песочницы. |
| `POOL_MIN_SIZE` | `1` | Минимум прогретых подов-песочниц. |
| `POOL_MAX_SIZE` | `3` | Максимум подов в пуле. |
| `LOG_LEVEL` | `INFO` | Уровень логирования Python. |

### Образы песочниц

В каждом образе песочницы есть:

- непривилегированный пользователь sandbox (UID / GID 64535),
- предустановленные библиотеки для языка,
- кэши, переехавшие в /tmp (у подов-песочниц нет постоянного хранилища),
- прогретые кэши для компилируемых языков, чтобы избежать холодного старта под CPU-лимитом песочницы.

Все песочницы рассчитаны на работу офлайн — сетевой доступ в рантайме запрещён Cilium-политикой, поэтому все зависимости должны быть вшиты в образ.

### Dockerfile.python

База: harbor.neoflex.ru/dognauts/python:3.10-slim-bookworm.

**Предустановленные библиотеки: numpy, pandas, requests**

Также заранее создаётся симлинк /tmp/.sandbox-venv/bin/python, чтобы llm-sandbox мог создать изолированный virtualenv без выхода в сеть.

### Dockerfile.javascript

База: harbor.neoflex.ru/dognauts/node:22-bookworm.

**Предустановленные библиотеки: axios, lodash, date-fns**

NPM_CONFIG_CACHE перенесён в /tmp/npm-cache, включён офлайн-режим (NPM_CONFIG_OFFLINE=true, NPM_CONFIG_PREFER_OFFLINE=true), чтобы npm install в рантайме не пытался достучаться до реестра.

### Dockerfile.typescript

База: harbor.neoflex.ru/dognauts/node:22-bookworm.

**Предустановленные библиотеки: typescript (глобально), axios, lodash, date-fns**

Офлайн-режим настроен так же, как в JavaScript-образе.

### Dockerfile.golang

База: harbor.neoflex.ru/dognauts/golang:1.23.

**Предустановленные модули: github.com/pkg/errors, gopkg.in/yaml.v3, github.com/spyzhov/ajson**

GOPROXY=off, GOSUMDB=off — Go отказывается выходить в сеть в рантайме.

Задержка холодного старта: без прогретого кэша сборки стандартная библиотека Go (fmt, encoding/json и т.д.) перекомпилируется с нуля при первом вызове execute_code в каждом свежесозданном поде-песочнице. Под CPU-лимитом 500m это занимает десятки секунд и приводит к таймаутам клиента. Чтобы этого избежать, образ заранее компилирует небольшую прогревную программу на этапе сборки, наполняя GOCACHE=/tmp/go-build-cache. **Не удаляйте шаг прогрева.**