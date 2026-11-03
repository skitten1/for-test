# Образ оператора StarRocks с патчем seccompProfile (image/seccomp.patch).
# Форк не нужен: исходники берутся из архива тега upstream, патч накладывается при сборке.
#
# Сборка (из папки image/):
#   docker build -t harbor.neoflex.ru/dognauts/starrocks/operator:v1.11.8-dognauts.1 .
#   docker push harbor.neoflex.ru/dognauts/starrocks/operator:v1.11.8-dognauts.1
#
# Другая версия оператора: --build-arg OPERATOR_VERSION=v1.11.9 (патч проверить: git apply --check).
# Закрытый контур: скачать архив тега заранее, положить рядом как src.tar.gz и заменить ADD на COPY src.tar.gz /tmp/src.tar.gz;
# базовые образы подменить через --build-arg GO_IMAGE=... RUNTIME_IMAGE=... (зеркало Harbor).

ARG GO_IMAGE=golang:1.22
ARG RUNTIME_IMAGE=starrocks/static-debian11:nonroot

FROM ${GO_IMAGE} AS build
ARG OPERATOR_VERSION=v1.11.8
ARG LDFLAGS
WORKDIR /src
ADD https://github.com/StarRocks/starrocks-kubernetes-operator/archive/refs/tags/${OPERATOR_VERSION}.tar.gz /tmp/src.tar.gz
RUN tar xzf /tmp/src.tar.gz --strip-components=1 -C /src
COPY seccomp.patch /tmp/seccomp.patch
# git есть в образе golang (в нём нет утилиты patch); git apply работает и без репозитория
RUN git apply --verbose /tmp/seccomp.patch
# в архиве тега есть каталог vendor/ — Go-модули из интернета не качаются
RUN CGO_ENABLED=0 GOOS=linux go build -mod=vendor -ldflags="${LDFLAGS:-}" -o /app/sroperator cmd/main.go

FROM ${RUNTIME_IMAGE}
COPY --from=build /app/sroperator /sroperator
CMD ["/sroperator"]
