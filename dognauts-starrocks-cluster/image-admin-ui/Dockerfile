# Сборка собственного образа StarRocks Admin UI.
# Исходный код берётся из закреплённого upstream-архива, НЕ из git-репозитория.
# Контекст сборки: каталог image-admin-ui, рядом должны лежать:
#   Dockerfile, src.tar.gz, dognauts.patch, apply_frontend_changes.js

ARG NODE_IMAGE=node:18-bookworm
ARG RUST_IMAGE=rust:1.89.0-bookworm
ARG RUNTIME_IMAGE=debian:bookworm-slim

# 1. Распаковка upstream-архива и применение backend-изменений.
FROM ${RUNTIME_IMAGE} AS source
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
    ca-certificates tar python3 \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY src.tar.gz /tmp/src.tar.gz
RUN tar -xzf /tmp/src.tar.gz --strip-components=1 -C /src
COPY apply_backend_changes.py /tmp/apply_backend_changes.py

RUN python3 /tmp/apply_backend_changes.py \
    /src/backend/src/middleware/auth.rs

# 2. Сборка Angular. Скрипт меняет frontend непосредственно в распакованном
#    исходном дереве; никаких git checkout/apply и клонирования репозитория.
FROM ${NODE_IMAGE} AS frontend
WORKDIR /src
COPY --from=source /src /src
COPY apply_frontend_changes.js /tmp/apply_frontend_changes.js
RUN node /tmp/apply_frontend_changes.js /src
WORKDIR /src/frontend
ENV PUPPETEER_SKIP_DOWNLOAD=true
RUN npm ci \
 && npm run build -- --configuration production --base-href ./

# 3. Сборка Rust backend с уже встроенным Angular dist.
FROM ${RUST_IMAGE} AS backend
WORKDIR /src
COPY --from=source /src/backend ./backend
COPY --from=frontend /src/frontend/dist ./frontend/dist
WORKDIR /src/backend
# В upstream-архиве Cargo.lock отсутствует. Генерируем lock в процессе
# сборки, затем используем --locked в самой сборке.
RUN cargo generate-lockfile \
 && cargo build --release --locked

# 4. Runtime-образ. Повторяет UID/GID и каталоги исходного Dognauts-образа.
FROM ${RUNTIME_IMAGE} AS runtime
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && groupadd -g 1001 starrocks \
 && useradd -u 1001 -g 1001 -r -s /usr/sbin/nologin starrocks

WORKDIR /app
RUN mkdir -p bin conf data logs
COPY --from=backend /src/backend/target/release/starrocks-admin ./bin/starrocks-admin
COPY --from=backend /src/backend/migrations ./migrations

RUN printf '%s\n' \
  '[server]' 'host = "0.0.0.0"' 'port = 8080' '' \
  '[database]' 'url = "sqlite://data/starrocks-admin.db"' '' \
  '[auth]' 'jwt_expires_in = "24h"' '' \
  '[logging]' 'level = "info"' 'file = "logs/starrocks-admin.log"' '' \
  '[metrics]' 'interval_secs = "30s"' 'retention_days = "7d"' 'enabled = true' '' \
  '[audit]' 'database = "starrocks_audit_db__"' 'table = "starrocks_audit_tbl__"' \
  > ./conf/config.toml \
 && chown -R 1001:1001 /app/data /app/logs \
 && chmod 0755 ./bin/starrocks-admin

USER 1001:1001
EXPOSE 8080
CMD ["./bin/starrocks-admin"]
