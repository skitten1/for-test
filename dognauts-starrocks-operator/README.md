# Dognauts platform chart for StarRocks operator

Обёртка над официальным чартом `operator` из https://github.com/StarRocks/starrocks-kubernetes-operator, release v1.11.8.
Ставится **один раз на кластер** через ArgoCD (Application добавляется в чарт-инсталлер платформы), как `dognauts-kmcp-controller`.

## Что входит
- Оператор StarRocks (Deployment, RBAC). Следит за всеми namespace: кластеры StarRocks создаются в проектах вложенным релизом `dognauts-starrocks`.
- CRD `starrocksclusters.starrocks.com` — из каталога `crds/` upstream-чарта. CRD кластерные, поэтому живут здесь, а не в чарте проекта.
- CiliumNetworkPolicy (`networkPolicy.type: cilium`).

## Подготовка
- Зеркалировать чарт `operator` 1.11.8 (`https://starrocks.github.io/starrocks-kubernetes-operator`) в `oci://harbor.neoflex.ru/dognauts-helm/starrocks`.
- **Образ оператора — свой, с патчем.** Оператор StarRocks не выставляет `seccompProfile` подам FE/BE, а политики платформы его требуют. Поле задаётся в Go-коде оператора, через values чарта его не изменить. Патчится не чарт (upstream-чарт `operator` остаётся в Harbor как есть), а образ: в папке `image/` лежат `Dockerfile` и `seccomp.patch` (4 строки в `pkg/k8sutils/templates/pod/spec.go`). Сборка скачивает архив исходников тега upstream, накладывает патч и собирает бинарник; **форк не нужен**, исходники оператора в нашем GitLab не хранятся. Папка `image/` не входит в пакет чарта (`.helmignore`).
  ```bash
  cd image
  docker build -t harbor.neoflex.ru/dognauts/starrocks/operator:v1.11.8-dognauts.1 .
  docker push harbor.neoflex.ru/dognauts/starrocks/operator:v1.11.8-dognauts.1
  ```
  При обновлении оператора: `--build-arg OPERATOR_VERSION=<новый тег>` и проверка, что патч накладывается (`patch -p1 --dry-run`). Если вместо патча хватит политики мутации кластера (Kyverno/Gatekeeper), образ можно не собирать. Юнит-тесты оператора (`spec_test.go`) патч не затрагивает: тесты в сборку образа не входят.

## Изменения в оригинальных файлах
- Чарт: нет, все настройки через values.
- Образ оператора: сборка с патчем (`image/`, см. выше) — `seccompProfile: RuntimeDefault` в securityContext пода и контейнеров FE/BE/CN.

## Примечания
- CRD из `crds/` Helm ставит только при первой установке и не обновляет. ArgoCD применяет их при каждой синхронизации, так что обновление версии CRD при апгрейде оператора проходит через Argo.
- Оператор подключается к FE кластеров StarRocks по MySQL-протоколу (9030). Протокол MySQL несовместим с подключениями из-за пределов mesh к подам в mesh, поэтому **namespace оператора должен быть в mesh** (метка инъекции sidecar; в Argo Application — `managedNamespaceMetadata`, см. `namespaceLabels` в блоке инсталлера). Сетевой политике оператора тогда нужен выход к istiod.
