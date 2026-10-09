# Russian and Ukrainian user guide: review notes

For a native speaker (or a second opinion). Both guides were translated from the English guide by an AI translator and have not been reviewed by a native speaker. Pull request: https://github.com/esoltys/luminous/pull/1594

- Files: `docs/user-guide/luminous-user-guide-RU.html` and `luminous-user-guide-UK.html`.
- To read them as the app shows them: set the UI language to Русский or Українська and open Help. Or run `bun run sync-docs` and open `static/luminous-user-guide-RU.html` in a browser.
- Terminology source: the app's own catalogs, `src/lib/locales/ru.ts` and `uk.ts`. Anything in **bold** in the guide that names a button, setting or view is copied from the catalog so it matches the screen; please leave those exactly as they are. Reviewer questions are about the surrounding prose, and about the words listed below that have no catalog string.
- House rules: `docs/TRANSLATIONS.md`, sections "ru" and "uk". Russian: polite «вы», no «ты», «ёлочки», ё where it disambiguates. Ukrainian: polite plural imperative, native terms over Russian-derived ones, typographic apostrophe ’.

## 1. Choices that are the translator's, not catalog strings

| English | Russian | Ukrainian | Question |
| --- | --- | --- | --- |
| chart (Top Albums "back on the chart") | чарт | рейтинг | Natural for a music app? Ukrainian could also be «хіт-парад» or «чарт». |
| heatmap | тепловая карта | теплова карта | Standard term for this chart? |
| chip (genre sub-genre chip) | метка | чип | «чип» may read as electronics; alternatives «мітка», «тег». |
| handle (drag handle on a card) | маркер | маркер | Fine, or «ручка»? |
| Take the tour (heading) | Пройдите обзор | Пройдіть огляд | The app's button says «Пройти краткий обзор» / «Швидкий огляд»; the guide heading is shorter. Fine as a heading? |
| About & Credits (nav title) | О программе и благодарности | Про програму та подяки | The catalog tab is only «О программе» / «Про програму»; the "credits" half is the translator's. |
| Add-on Supporter Themes heading | Темы-дополнения для сторонников (только версия из Microsoft Store) | Теми для прихильників (доповнення), лише у версії Microsoft Store | Parenthetical wording. |
| Edit (lyrics) | Изменить | Редагувати | No catalog string for the lyrics Edit button; Russian and Ukrainian differ on purpose, but the Ukrainian artist page uses «Змінити». Pick one verb for Ukrainian? |
| Turn on **Lyrics** in the player bar | Включите **Тексты песен** | Увімкніть **Тексти пісень** | Both now use the sidebar name from the catalog. The player-bar button's own tooltip is «Показать панель текста» / «Показати панель тексту»; is the sidebar name the right one to quote here? |
| Right-click the tray icon for … | paraphrased | «Відтворення/Пауза», «Наступна пісня», «Попередня пісня» | The Ukrainian quotes the new tray menu labels (from the tray fix in PR #1593). |
| Portable Mode sentences | «работает в переносном режиме (метка **Переносной режим**)» | «працює як **Портативний режим**» | Both reworded so the bold label stays in its catalog form; the Ukrainian reads a bit odd. |
| Predefined themes | «из раздела **Готовые темы**» | «в розділі **Готові теми**» | The catalog only has the heading «Готовые/Готові теми», so the sentence says "section" rather than quoting a button. |
| Verb "click" | Нажмите / Щёлкните | Клацніть | Ukrainian uses «Клацніть» for clicks everywhere; Windows Ukrainian says «Клацніть» or «Натисніть»? |

## 2. Sentences that were restructured around an icon or link

The English sentences have an icon or a link in the middle, so each is translated in two pieces. Read these in the browser to check they flow.

- Playback section: the button that switches the waveform to frequency bands ("beside it switches to frequency bands…"). The icon comes before the sentence.
- Playback section: "the buttons on the right open: the song menu, lyrics, the Queue, the info panel, the mini player". Russian uses the accusative («очередь», «информационную панель»), Ukrainian likewise.
- Advanced Organize: "Choose Custom on the Organize page." (Russian now: «Чтобы написать собственный шаблон, выберите пункт **Свой** на странице [Упорядочение].»)
- Equalizer: the AutoEq link in "Find your headphones on AutoEq, choose …".
- About: the GitHub link in "The source code is on GitHub, with the credits for every library…".

## 3. Things worth a general read

- Do the keyboard shortcut descriptions (last "Tips and tricks" section) read as natural short captions?
- Keycaps: `Ctrl`, `Shift`, `Esc`, `Page Up` and `Page Down` are left in English on purpose, because Russian and Ukrainian keyboards print them that way. Only `Space` is translated (`Пробел` / `Пробіл`). Say so if your keyboards differ.
- Search syntax examples keep English field names (`genre:rock`) on purpose: the search syntax is English. Check the explanations around them make that clear.
- Plural and case agreement in sentences with numbers («до 25 песен», «хвилини», …).
- Any place that sounds like machine translation, or uses a Russian-derived word in the Ukrainian guide.

## 4. Where to send feedback

Comment on the pull request, or edit the HTML directly (the text is plain, one element per sentence) and open a pull request. For catalog-string changes edit `src/lib/locales/ru.ts` / `uk.ts` and keep the guide in step.

## 5. Outcome of the first review (PR #1594 comment)

Applied:
- UK: «чип» → «мітка»; «рейтинг» → «чарт» (three places); lyrics **Edit** → «Змінити» (the app's `settings.editThemeShort`); «збігається 25 пісень» → «щойно набереться 25 відповідних пісень»; «додати її» → «додати їх»; add-on themes heading restructured; "Take the tour" heading → «Швидкий огляд» (the catalog's wording); Portable Mode sentence («використовує»).
- RU: tray sentence now quotes the tray menu labels; Portable Mode sentence («использует»); «Кнопка» added before the waveform/frequency sentence; «показатели» added in the heatmap sentence; «Включите параметр …» for the equalizer switch; auto-organize sentence («новые файлы и файлы с изменёнными тегами»); GitHub/credits sentence simplified; "Take the tour" heading → «Краткий обзор».
- ES, IT, UK: the search syntax example keeps the English `field:value`, because the search only accepts English field names. (The French guide is fixed too, in #1590.)

Kept as they were, on purpose:
- Tray labels «Відтворення/Пауза» / «Воспроизведение/Пауза» keep no spaces around the slash. The shortcut list in the app writes «Відтворення / пауза» with spaces, so the two places differ slightly; unify them in the catalog if you want them identical.
- The keyboard-shortcut captions in the last section are copied from the app's own shortcut list (`shortcuts.*`), so they match what the app shows even where they are wordier.
- «чарт» (RU), «тепловая/теплова карта», «метка» (RU), «маркер», «О программе и благодарности», «Готовые/Готові теми»: confirmed natural by the reviewer.
