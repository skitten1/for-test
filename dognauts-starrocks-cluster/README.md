# dognauts-starrocks-cluster

StarRocks (MPP SQL-движок) как **вложенный релиз** проекта 2.0 (keyword `dognauts-nested-release`). Один экземпляр на проект. Вместе с кластером ставится **StarRocks Admin UI**.

Оператор и CRD — отдельный чарт `dognauts-starrocks-operator` (ArgoCD, один на кластер Kubernetes). Здесь — только экземпляр: CR `StarRocksCluster` (upstream-чарт `starrocks`) и обвязка платформы.

## Состав

| Что | Файл | Зачем |
|---|---|---|
| `StarRocksCluster` `starrocks` (FE + BE) | upstream `starrocks` (зеркало `1.11.8-dognauts.1`) | сам кластер |
| Служебные headless-сервисы портов FE/BE | `internal-services.yaml` | Istio шифрует внутренний трафик FE↔BE (mTLS) |
| Джобы Vault (pre-install/upgrade, pre-delete) | `vault-jobs.yaml` | генерация недостающих секретов в Vault, удаление вместе с релизом |
| Init-джоба (post-install/upgrade) | `init-job.yaml` | пароль root, служебные пользователи StarRocks, каталог Iceberg (Lakekeeper) |
| Прокси консоли FE (nginx) | `console-proxy.yaml` | автовход в консоль FE и работа под префиксом платформы |
| Admin UI (Deployment, Service, PVC) | `admin-ui.yaml` | удобная админка StarRocks |
| Настройка Admin UI (post-install/upgrade) | `admin-ui-init-job.yaml` | пароль admin из Vault, регистрация кластера проекта |
| VirtualService | `virtualservice.yaml` | `…/starrocks/` → консоль, `…/starrocks/admin-ui/` → Admin UI, регистрация в Admin UI закрыта |
| ext_authz, логгер | `envoyfilter.yaml`, `logger-envoyfilter.yaml` | Keycloak + Casbin на прокси консоли и Admin UI |
| Меню платформы | `_project-component.yaml`, `project-component.yaml` | два пункта: консоль FE и Admin UI |
| Сетевые политики Cilium | `values.yaml` → `networkPolicy`, `ciliumnetworkpolicy.yaml` | все поды и джобы |

## Секреты (Vault)

Секреты лежат в Vault по соглашению платформы (`…/projects/<проект>/starrocks` и `…/starrocks-admin-ui`). Поды получают **только нужные ключи** через webhook Bank-Vaults (значения `vault:<путь>#<ключ>` в переменных окружения).

| Ключ | Путь | Кто использует |
|---|---|---|
| `MYSQL_PWD` | `starrocks` | FE, BE (скрипты запуска), init-джоба — пароль root |
| `STARROCKS_CONSOLE_PASSWORD` | `starrocks` | init-джоба (пользователь `platform_console`) |
| `STARROCKS_CONSOLE_BASIC` | `starrocks` | прокси консоли |
| `APP_JWT_SECRET` | `starrocks-admin-ui` | Admin UI |
| `APP_PLATFORM_AUTOLOGIN_PASSWORD` | `starrocks-admin-ui` | Admin UI, джоба настройки (пользователь приложения `admin`) |
| `STARROCKS_ADMIN_UI_SR_PASSWORD` | `starrocks-admin-ui` | init-джоба, джоба настройки (пользователь StarRocks `platform_admin_ui`) |

Пароли генерирует джоба `vault-ensure` **только если ключа ещё нет**: значения не меняются при обновлениях и не появляются в манифестах Helm. При удалении релиза секреты удаляются (`vault.deleteOnUninstall`), как и тома FE/BE.

## Подготовка

1. **Оператор** — чарт `dognauts-starrocks-operator`; образ оператора собран с патчем seccompProfile `RuntimeDefault` для подов FE/BE (папка `image/` чарта оператора, инструкция — в его README).
2. **Upstream-чарт `starrocks` 1.11.8** с патчем (`storageClassName` без `null`) → `oci://harbor.neoflex.ru/dognauts-helm` как `1.11.8-dognauts.1`.
3. **Образы в Harbor** (`<DOCKER_IMAGE_REGISTRY>/…`):
   - `starrocks/fe-ubuntu:4.1.7`, `starrocks/be-ubuntu:4.1.7`;
   - `nginx-unprivileged:1.27-alpine` (из `nginxinc/nginx-unprivileged`);
   - `starrocks-admin:1.0.0-dognauts.1` — собирается из папки `image-admin-ui/` этого чарта (Dockerfile, патч Dognauts и закреплённый `Cargo.lock`); официальный образ автора и его helm-чарт не используются, форк не нужен. Инструкция — в комментарии в начале Dockerfile.
4. **authz-service и Casbin:** маска пути `starrocks` и политики (покрывают и консоль, и Admin UI — Admin UI живёт внутри префикса `…/starrocks/admin-ui/`).
5. `helm dependency update`, `helm schema`, `helm lint`.

## Подключение

- SQL (MySQL-протокол) внутри namespace проекта: `starrocks-fe-service:9030`. Снаружи кластера — `kubectl port-forward`.
- Консоль FE и Admin UI — пункты меню проекта, вход автоматический (доступ проверяет платформа).
- Каталог Iceberg: `lakekeeperCatalog.enabled: true` + `uri`, `warehouse`, `s3.credentialsSecret` (секрет с ключами S3 проекта). StarRocks не умеет remote signing Lakekeeper, поэтому ходит в S3 со своими ключами.

## Безопасность

- securityContext у всех подов и джоб: `runAsNonRoot`, `allowPrivilegeEscalation: false`, `capabilities.drop: ALL`, `seccompProfile: RuntimeDefault` (у FE/BE — через патч оператора), `readOnlyRootFilesystem: true` (у FE/BE каталоги для записи вынесены на emptyDir, conf копируется init-контейнером).
- Весь трафик между подами — через Istio с mTLS, включая SQL и внутренние порты FE/BE. Init-джобы и оператор работают в mesh.
- Через шлюз в StarRocks попадают только через прокси консоли и Admin UI, оба под ext_authz (Keycloak + Casbin).
- Секреты — в Vault, каждый под получает только свои ключи.
- Образы — только из Harbor.

## Открытые вопросы / TODO

- [ ] Роль Vault: убедиться, что под ней разрешены и SA обычных подов (FE, BE, прокси консоли, Admin UI), а не только `*-vault-jobs`.
- [ ] `readOnlyRootFilesystem` FE/BE на 4.1.7 — проверить; при `Read-only file system` добавить путь в `emptyDirs`.
- [ ] Каталог Iceberg при включённом ext_authz у Lakekeeper: запросам FE к Lakekeeper нужен токен или исключение в authz-service.
- [ ] SQL-доступ пользователей: учётка проекта для ноутбуков; SSO через Keycloak (JWT в StarRocks 3.5+).
- [ ] `slug` в старом формате фронта; новый — `<релиз>/starrocks`.
- [ ] Квота проекта: минимальная конфигурация (FE + BE + прокси + Admin UI + sidecar'ы) требует ~3.5 ГБ памяти запросов.
