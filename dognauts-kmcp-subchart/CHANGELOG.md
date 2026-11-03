# Changelog

Все значимые изменения в проекте документируются в этом файле.

Changelog ведется на основе принципов [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) и [семантического версионирования](https://semver.org/spec/v2.0.0.html).

## Unreleased

### Fixed
 - Исправлен баг обработки url к репозиториям с портом.
 - Добавлено подключение AzureRepo

## [0.4.0]
### Added
- Добавлены sandbox на javascript, typescript
- добавлена job mcp-cleaner, удаляющая релизы МСР при удалении nested-чарта (NFFDMLOPSCENTERDEV-6157)

### Changed
- Скорректирована cilium-политика: добавлена политика для пода-песочницы с лейблом llm-sandbox, запрещающая любой трафик вовне

### Fixed
- устранены следующие баги: ошибка 500 при удалении релиза МСР (NFFDMLOPSCENTERDEV-6182), задержка создания пода-песочницы (NFFDMLOPSCENTERDEV-6184)

## 0.3.0

### Added

- добавлено создание Telemetry с помощью хелпера dognauts-common-chart
