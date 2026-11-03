# Пересборка образа StarRocks Admin UI

## Что именно меняем

Меняется только содержимое образа `starrocks-admin`. Исходный репозиторий `starrocks-admin` клонировать, форкать или коммитить не нужно. Dockerfile получает архив конкретного upstream-коммита, распаковывает его, применяет backend-патч и выполняет frontend-преобразования, затем собирает Angular и Rust и создаёт новый runtime-образ.

Не путать:
- upstream-исходники внутри архива — входные данные для сборки образа;
- `dognauts.patch` и `apply_frontend_changes.js` — инструкции изменения этих исходников только внутри Docker build;
- родительский Helm-чарт — отдельно разворачивает образ, PVC, Service, provisioner и Vault.

## Подготовка каталога сборки

Сложи в один каталог четыре файла:

- `Dockerfile`
- `dognauts.patch`
- `apply_frontend_changes.js`
- `src.tar.gz`

Архив должен быть исходниками `jlon/starrocks-admin` на коммите:

`e8129b1444b86f6a2c87a5d08798cfde61c7d197`

Скачать архив в WSL из этого каталога:

```bash
curl -fL \
  https://github.com/jlon/starrocks-admin/archive/e8129b1444b86f6a2c87a5d08798cfde61c7d197.tar.gz \
  -o src.tar.gz
```

Репозиторий git для этого не нужен. `git apply`, `git checkout`, `git clone` и коммиты не используются.

## Сборка

Выполнять из каталога, где лежат четыре файла:

```bash
docker build --pull \
  --build-arg NODE_IMAGE=node:18-bookworm \
  --build-arg RUST_IMAGE=rust:1.89.0-bookworm \
  --build-arg RUNTIME_IMAGE=debian:bookworm-slim \
  -t harbor.neoflex.ru/dognauts/starrocks-admin:1.0.0-dognauts.2 \
  .
```

Dockerfile сам:
1. распакует `src.tar.gz`;
2. применит `dognauts.patch` к backend;
3. запустит `apply_frontend_changes.js`;
4. соберёт frontend и backend;
5. сформирует runtime-образ с UID/GID 1001.

В сборочной среде нужны доступ к Docker Hub (или зеркалам базовых образов), npm registry и crates.io. Для сборки Rust выдели Docker не менее 4 ГБ памяти.

## Локальная проверка

Запусти контейнер с теми же ограничениями, что и в Deployment:

```bash
docker run --rm --name starrocks-admin-check \
  -p 8080:8080 \
  --user 1001:1001 --read-only --cap-drop ALL \
  --tmpfs /tmp \
  -v sr-admin-data:/app/data -v sr-admin-logs:/app/logs \
  -e APP_JWT_SECRET='local-test-only' \
  harbor.neoflex.ru/dognauts/starrocks-admin:1.0.0-dognauts.2
```

Проверь доступность:

```bash
curl -i http://localhost:8080/
```

Здесь не надо проверять `/api/auth/platform-login`: в новом варианте этого endpoint нет. Проверка ручного `/api/auth/login` имеет смысл только после provisioner/bootstrap с известным паролем.

## Публикация

```bash
docker push harbor.neoflex.ru/dognauts/starrocks-admin:1.0.0-dognauts.2
```

Не перезаписывай `.1`: новый тег позволяет откатиться.

## Что менять в родительском Helm-чарте

### `values.yaml`

Нужно изменить только тег образа:

```yaml
adminUi:
  image:
    repository: "[@< .Env.DOCKER_IMAGE_REGISTRY >@]/starrocks-admin"
    tag: "1.0.0-dognauts.2"
```

Остальные параметры `adminUi` оставить без изменений.

### `templates/admin-ui.yaml`

Для перехода на новый образ файл не требуется менять. PVC, основной контейнер, provisioner, его `127.0.0.1:8080/api`, Vault-переменные и securityContext остаются как есть.

Переменные `APP_PLATFORM_AUTOLOGIN_USER` и `APP_PLATFORM_AUTOLOGIN_PASSWORD` в основном контейнере новый образ не использует. Их можно удалить отдельной необязательной уборкой, но для работы нового образа это не требуется. Не удаляй `APP_USER`/`APP_PASSWORD` из provisioner и не меняй его логику.

### VirtualService — отдельная проверка

Frontend нового образа строит API URL под полным префиксом текущего Admin UI:

`/project-api/<project>/<release>/starrocks/admin-ui/api/...`

Нужно убедиться, что существующий VirtualService уже маршрутизирует этот префикс на `starrocks-admin-ui:8080` с rewrite на `/api/...`. Если сейчас в нём есть только общий маршрут `/project-api/api/`, одного изменения `values.yaml` недостаточно: потребуется поправить именно шаблон VirtualService (не `admin-ui.yaml`). Это изменение маршрутизации, а не изменение образа. Не выкатывай, пока не проверен путь, который фактически получает ext_authz после rewrite.

## Проверка после развёртывания

В DevTools → Network при обычном входе из меню не должно быть запросов:

- `/api/auth/platform-login`
- `/api/auth/login`
- `/api/auth/permissions`

Provisioner должен продолжить работать в том же Pod и через `127.0.0.1`; его задача — инициализировать SQLite-пользователя `admin`, сменить начальный пароль и зарегистрировать StarRocks cluster. Этот механизм не удаляется.

Платформенный режим backend доверяет, что входящий `Authorization: Bearer ...` уже проверен ext_authz. Поэтому все внешние запросы к Admin UI должны проходить через защищённый gateway/sidecar; прямой незащищённый доступ к Service недопустим.
