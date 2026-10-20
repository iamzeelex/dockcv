# cv-application-tracker

Конечный автомат и журнал жизненного цикла заявок на вакансии (Job Applications Tracker).

Позволяет агенту или пользователю детерминированно фиксировать отклики, менять статусы между стадиями воронки (`wishlist` → `applied` → `interviewing` → `offer` / `closed`), логировать раунды интервью и следующие шаги с датами, а также рассчитывать конверсию воронки.

Капсула класса `morphic` / `atom`: чистая функция над состоянием $(s', o) = f(s, i)$, без собственного скрытого ввода-вывода или чтения настенных часов. Текущая дата передаётся параметром запроса.

<!-- facts:begin (generated for Kraton catalog) -->

| | |
|---|---|
| Модуль | `cv-application-tracker` 0.1.0 |
| Вид | morphic / atom: переход состояния и метрики |
| Контракт | `wit/application-tracker.wit`, world `application-tracker-vmc` |
| Экспорты | `cv:application-tracker/tracker@0.1.0` (`add-application`, `advance-application`, `log-interview`, `schedule-step`, `parse-board`) |
| Импорты | нет |
| Сборка | `cargo build --target wasm32-wasip2 --release` |
| Компонент | `target/wasm32-wasip2/release/cv_application_tracker.wasm` |

<!-- facts:end -->

## Операции

1. **`add-application`**: Добавление новой карточки в вишлист или поданные заявки с фиксацией текста вакансии, зарплатных ожиданий и привязанной версии резюме.
2. **`advance-application`**: Продвижение карточки по стадиям воронки с автоматической фиксацией хронологии переходов (`history`) и причины закрытия (`closed_as: rejected | ghosted | withdrew | declined | accepted`).
3. **`log-interview`**: Запись состоявшегося собеседования (скрининг, техническое интервью, онсайт, оффер).
4. **`schedule-step`**: Планирование следующего дедлайна или действия (дата, время, описание).
5. **`parse-board`**: Разбор TOML-доски и расчет агрегированных метрик воронки конверсий.
