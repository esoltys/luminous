# Translating the Luminous UI

This covers the in-app UI strings only. The user guide ships in English and French, and Store listings are not localized per language. All UI languages are produced and maintained in this repo; outside translation contributions are not accepted as a workflow.

## Adding a locale

1. Add `src/lib/locales/<code>.ts` exporting a `Messages` object with exactly the keys of `en.ts` (no missing keys, no stale keys).
2. Add one entry to `LOCALES` in `src/lib/locales/index.ts`: a BCP 47 `tag`, the `messages`, and an optional `fallback` tag. The picker label is derived from the tag via `Intl.DisplayNames`, so don't hand-write it. `Locale`, the Settings list and the saved-setting check all derive from `LOCALES`.
3. Regional variants (en-US, en-GB, fr-FR) set `fallback` to the language they share a base with, so a variant file only has to contain the keys that differ. A variant is only worth shipping if it has real differences to carry.
4. Run `bun run test:run src/lib/locales/locales.test.ts`. That gate enforces key completeness, matching `{placeholder}` tokens, and non-identical text versus English. A string that is legitimately the same as English (a loanword, an acronym, a brand, a symbol) goes in `IDENTICAL_OK` with a comment.
5. Consult the terminology authority for the language (below) before choosing a term for anything that isn't obvious, and add any term you had to look up to that language's established-terms list.

## Voice and register

- **Instructive, not descriptive.** Hint and tooltip text tells the user what to do or what happens when they act ("Scan watched folders for new files"), in the imperative voice of the control's own label. It does not describe the feature in third person or lead with an adverb. Apply the rule in each language with that language's natural instruction form; do not carry English grammar across.
- **One register per language, used everywhere.** Pick it once and keep it: informal imperative ("tu"/"du"/"ти") versus formal ("vous"/"Sie"/"Ви") is the main decision. Record it in the language's section below when the first draft is made.
- **Translate meaning, not words.** Avoid calques of English UI idiom. Prefer the term users already see in Windows, as listed in the Microsoft Terminology Collection.
- **Section headings and status labels** stay descriptive and short; sentence case unless the language's convention says otherwise.
- **Proper nouns and brands stay as they are:** Luminous, MusicBrainz, ListenBrainz, Last.fm, Discord, Subsonic, LRCLIB and so on.

## Placeholders and counts

- Keep every `{token}` exactly as written, including its name. Word order around it can change; a token cannot be dropped, renamed or duplicated.
- Counts: a string whose number agrees with a noun is a plural object keyed by CLDR category, not a pair of keys: `songsCount: { one: "{count} song", other: "{count} songs" }`. Callers use `i18n.plural("playlists.songsCount", n)`; the category comes from `Intl.PluralRules` for the locale, so French counts 0 as singular and Ukrainian or Russian use `one` / `few` / `many` / `other` (1 файл, 2 файли, 5 файлів). Supply every category the locale uses for whole numbers (`locales.test.ts` checks this against `Intl.PluralRules`); `other` is the runtime fallback for a category you leave out, such as `many` for multiples of a million in French, Spanish and Italian. Every form keeps the same `{token}` set as English, including `{count}`, which is already locale-formatted.
- A string with several counts, or a count that does not sit beside its noun, cannot use one plural key. Rephrase it around the numbers ("Cleaned up missing songs: {count}; merged duplicates: {duplicates}") so nothing needs to agree.
- Keep numbers, dates and durations out of the string when the UI already formats them with `formatNumber` or `Intl`; the token receives the locale-formatted value.

## Strings the backend stores in English

The backend stores some auto-playlist names in English, and the UI shows a localized label derived from the playlist's `dynamic_spec`, so a missing translation shows up as English on a card, not as a missing key. Translate all of them in every language:

- **Moment Mix time of day:** `playlists.daypartMorning`, `daypartAfternoon`, `daypartEvening` and `daypartLateNight` (a full name such as "Afternoon Mix", not just the time word). The row's own `name` stays English; `getDaypartMixLabel` in `src/lib/utils/playlist.ts` picks the label. Keep the wording aligned with `stats.clock*` and `home.greeting*`, since the buckets are the same.
- **BPM buckets:** `playlists.bpmDownTempo` through `bpmExtreme`, via `getBpmBucketLabel`.

When checking a new locale in the running app, open Home and Playlists on an auto-playlist card and the global search results: these labels are the ones the completeness test cannot catch if a component falls back to an English default instead of a key.

## Terminology authorities

Each source is where a term gets checked, in the order listed. "Microsoft Terminology Collection" applies to every language, because most users meet these terms in Windows: <https://learn.microsoft.com/en-us/globalization/reference/microsoft-terminology>.

| Locale | Authority, in order | Notes |
| --- | --- | --- |
| fr-CA | OQLF Grand dictionnaire terminologique (<https://vitrinelinguistique.oqlf.gouv.qc.ca>), then TERMIUM Plus (<https://www.btb.termiumplus.gc.ca>) | In force today; see below. |
| fr (France) | FranceTerme, then the Académie française (<https://www.academie-francaise.fr>) | Where France and Canada differ, the variant file carries only the differences. |
| es | RAE/ASALE *Diccionario de la lengua española* and *Diccionario panhispánico de dudas*, then Fundéu (<https://www.fundeu.es>) | Neutral register that works across Latin America and Spain. |
| de | Duden (<https://www.duden.de>) | |
| it | Treccani (<https://www.treccani.it>), then the Accademia della Crusca (<https://accademiadellacrusca.it>) | |
| uk | *Український правопис* (2019 Ukrainian Orthography) | Prefer native Ukrainian terms over Russian-derived calques. |
| ru | Gramota.ru | |
| en-US | Merriam-Webster | |
| en-GB | Oxford English Dictionary or Collins | |

Link status when this was written: the hosts above that have a URL answered, except the RAE, Gramota.ru and Merriam-Webster sites, which refuse automated requests and need a manual check in a browser. No stable FranceTerme or Ukrainian Orthography URL has been confirmed yet; find and add them when the fr (France) locale is drafted and when the uk draft gets a native review. Treat every authority as "proposed" until the person drafting that language has used it once and confirmed it gives usable answers for UI terms.

## French (Canada)

French is Canadian French, for `fr.ts` and the French user guide.

- **Established terms:** *étiquette* (tag), *diffusion en continu* (streaming), *bogue* (bug), *liste de lecture* (playlist), *simple* (single), *zone de notification* (system tray), *palmarès* (chart), *image dans l'image* (picture-in-picture).
- **Typography:** a space before `:` and inside « », none before `?`, `!` or `;`.
- **Register:** formal "vous"; no "tu" in UI text.

## Other languages

Established terms and register are recorded here as each language is drafted, so the lessons are written down before the next language starts. Each entry is a term, its English source, and one line on why it was chosen over the obvious alternative.

- **it** (locale tag `it`; drafted blind, then diffed against the contributed file in #1421). **Register:** informal "tu" imperative everywhere ("Scegli", "Aggiungi", "Fai clic"); both drafts agreed. Established terms:
  - *brano* (song/track; *traccia* only where English says "Track #" or MusicBrainz track), *libreria* (library), *raccolta* (Collection view), *playlist* (kept; *playlist intelligente*, not "Smart Playlist"), *coda* (Queue), *preferiti* (favourites), *testi* (lyrics), *cartella* (folder, never "directory"), *unità* (drive, not "disco"), *intestazione* (header), *area di notifica* (system tray), *dissolvenza incrociata* (crossfade), *tag* (kept, as Windows does).
  - Failure: "non riuscito/a" ("Salvataggio non riuscito"), not the harsher "fallito" (24 uses in the contribution). Success: "correttamente", not the calque "con successo".
  - Verbs: "Fai clic", not the colloquial "Clicca". "Rilascia" for drop, not "Trascina" (which is drag).
  - Quotes: «guillemets» around substituted names.
  - Lessons from the diff, for every language: (1) don't drop articles and prepositions to mimic English headline style ("Fase scansione", "Conteggio riproduzioni"); write the natural phrase ("Fase della scansione", "Numero di riproduzioni"). (2) Don't leave English UI jargon that has an everyday word (*Smart Playlist*, *Builder*, *header*, *fallback*, *provider*, *tray*, *Top*): the contribution kept about 30. Keep a loanword only when Windows does. (3) No "(s)" slash plurals ("brano/i", "cartella/e"): rephrase around the count ("brani mancanti: {count}") so they read in both singular and plural. (4) Keep Title Case out of labels; Italian uses sentence case (the contribution had ~60% more title-cased labels). (5) Watch noun gender when a word is carried over ("Una player" for "Un lettore"). (6) Check the source snapshot: the contribution was built from an older `en.ts` (68 current keys missing, 10 stale ones such as `settings.languageItalie`), and its claim of passing `locales.test.ts` did not hold (a `;` for `,` at line 263 made the file unparseable). Always re-run the gate on a contributed catalog.
- **es** (locale tag `es`, one neutral Latin-American-friendly catalog; no `es-MX`/`es-ES` variants). **Register:** informal "tú" imperative everywhere ("Elige", "Agrega", "Haz clic"); never "usted". Established terms:
  - *canción* (song; *pista* only where English says "Track #" or MusicBrainz track), *biblioteca* (library), *lista de reproducción* (playlist; *lista inteligente* for Smart Playlist, *lista automática* for Auto-Playlist), *cola* (Queue), *favoritas* (favourites), *letras* (lyrics), *carátula* (cover art), *etiqueta* (tag), *carpeta* (folder), *unidad* (drive), *área de notificación* (system tray), *fundido cruzado* (crossfade), *tasa de bits* (bitrate), *recopilación* (compilation), *Configuración* (Settings, as Windows does), *computadora* (not *ordenador*).
  - Typography: «guillemets» around substituted names, a space before `%` ("{percent} %"), decimal comma ("0,5 s"), `n.º` for "#". Sentence case in labels; no "(s)" slash plurals, so rephrase around the count ("Canciones pendientes: {count}").
  - Kept English only where Windows or the field does: *Scrobbling*, *Bit-perfect*, *Pop/Rock/Jazz*, brands and symbols (see `IDENTICAL_OK_ES`).
  - Time-of-day wording matches across `home.greeting*`, `stats.clock*` and `playlists.daypart*` (*mañana*, *tarde*, *noche*, *madrugada*).
- **de** (locale tag `de`, one catalog for Germany, Austria and Switzerland; no regional variants). **Register:** avoid direct address where the language allows it (infinitive for buttons and hints: "Ordner hinzufügen", "Dateien öffnen"); where a sentence must address the user, informal "du" ("Wähle", "deine Mediathek"); never "Sie". Established terms:
  - *Titel* (song; *Titel-Nr.* for "Track #"), *Interpret* (artist; *Albuminterpret*), *Mediathek* (library, as Windows Media Player does), *Sammlung* (Collection view), *Wiedergabeliste* (playlist; *intelligente Wiedergabeliste* for Smart Playlist, *Auto-Wiedergabeliste* for Auto-Playlist (the longer "automatische" clipped on cards)), *Warteschlange* (Queue), *Favoriten* (favourites), *Songtexte* (lyrics), *Cover* / *Albumcover* (artwork), *Tag* (kept, as Windows does), *Ordner* (folder), *Laufwerk* (drive), *Infobereich* (system tray), *Überblenden* (crossfade), *Sampler* (compilation), *Einstellungen* (Settings), *Vorverstärkung* (preamp), *Glocke* (peak filter), *Editor* (builder), *Statistik* (Stats).
  - Typography: „Anführungszeichen“ around substituted names, a space before `%` and `dB`, decimal comma ("0,5 s", "4,5:1"), `Min.` for minutes, en dash with spaces as the sentence dash (" – "), "A–Z" sort ranges. Nouns are capitalised as German requires; labels are otherwise not Title Case.
  - Failure: "… konnte nicht gespeichert werden" or "Fehler beim …", not "fehlgeschlagen" for every case. No "(s)" slash plurals; rephrase around the count.
  - Kept English where Windows or the field does: *Scrobbling*, *Bit-perfect*, *Resampling*, *Codec*, *Decoder*, *Pop/Rock/Jazz*, *Add-on*, brands and symbols (see `IDENTICAL_OK_DE`).
  - Time-of-day wording matches across `home.greeting*`, `stats.clock*` and `playlists.daypart*` (*Morgen*, *Nachmittag*, *Abend*, *Nacht*).
- **uk** (locale tag `uk`; drafted from `en.ts` by Claude, not yet reviewed by a native speaker). **Register:** no direct address in labels, buttons and hints (infinitive: "Додати папку", "Вибрати мову меню, кнопок і повідомлень."); where a sentence must address the user, the polite plural imperative as Windows does ("Виберіть", "Натисніть", "Установіть"); never "ти". Established terms:
  - *пісня* (song; *композиція* for a track, *№ композиції* for "Track #"), *виконавець* (artist), *бібліотека* (library), *колекція* (Collection view), *список відтворення* (playlist; *розумний список відтворення* for Smart Playlist, *автосписок* for Auto-Playlist), *черга* (Queue), *улюблені* (favourites), *текст пісні* (lyrics), *обкладинка* (artwork), *тег* (tag), *папка* (folder; Windows says *папка*, so not the native-but-unfamiliar *тека*), *диск* (drive), *область сповіщень* (system tray), *перехресне згасання* (crossfade), *попереднє підсилення* (preamp), *Налаштування* (Settings), *Статистика* (Stats), *скроблінг* (scrobbling), *мікс* (mix), *доповнення* (add-on).
  - Plurals: every counted string has `one` / `few` / `many` / `other` (1 пісня, 2 пісні, 5 пісень); `other`, used for fractions, is the genitive singular. Where a verb or participle agrees with the number, the form is written out per category ("Відтворюється 1 пісня", "Відтворюються 2 пісні").
  - Typography: «лапки» around substituted names, the typographic apostrophe ’ (U+2019) in words like *пов’язано*, a space before `%` and units (`{percent} %`, `дБ`, `кБ`), decimal comma, `хв` for minutes and `с` for seconds, "А–Я" sort ranges. Sentence case in labels.
  - Time-of-day wording matches across `home.greeting*`, `stats.clock*` and `playlists.daypart*` (*ранок*, *день*, *вечір*, *ніч*). Short names matter in cards and tabs: auto-playlist and theme names were shortened after a width check (*Часто відтворювані*, *Морська деревина*, *Повільний BPM*).
  - Kept English where Windows or the field does: *BPM*, *Bit-perfect*, *Hi-Res Audio*, brands, symbols and example URLs (see `IDENTICAL_OK_UK`).
- **ru** (locale tag `ru`; drafted from `en.ts` by Claude, not yet reviewed by a native speaker). **Register:** no direct address in labels, buttons and hints (infinitive: "Добавить папку", "Выбрать язык меню, кнопок и сообщений."); where a sentence must address the user, the polite "вы" imperative as Windows does ("Выберите", "Нажмите", "Добавьте"); never "ты". Established terms:
  - *песня* (song; *композиция* for a track, *№ композиции* for "Track #"), *исполнитель* (artist), *библиотека* (library), *коллекция* (Collection view), *список воспроизведения* (playlist; *умный список воспроизведения* for Smart Playlist, *автосписок* for Auto-Playlist), *очередь* (Queue), *избранное* (favourites), *текст песни* (lyrics), *обложка* (artwork), *тег* (tag), *папка* (folder), *диск* (drive), *область уведомлений* (system tray), *плавный переход* (crossfade), *предусиление* (preamp), *Параметры* (Settings, as Windows does), *плейлист* (only in short headings, where *список воспроизведения* clips: the sidebar entry and view title), *Статистика* (Stats), *скробблинг* (scrobbling), *микс* (mix), *дополнение* (add-on), *НЧ-полка* / *ВЧ-полка* (low/high shelf).
  - Plurals: every counted string has `one` / `few` / `many` / `other` (1 песня, 2 песни, 5 песен); `other`, used for fractions, is the genitive singular. Where a verb or participle agrees with the number, the form is written out per category ("Показана 1 песня", "Показано 2 песни", "Показано 5 песен"); after *для* the genitive plural is used for 2 and up ("для 2 песен").
  - Typography: «ёлочки» around substituted names, *ё* written where it disambiguates (*всё*, *Тёмная*, *Жёлтый*), a space before `%` and units (`{percent} %`, `дБ`, `КБ`), decimal comma, `мин` for minutes and `с` for seconds, "А–Я" sort ranges, "№" for "#". Sentence case in labels.
  - Time-of-day wording matches across `home.greeting*`, `stats.clock*` and `playlists.daypart*` (*утро*, *день*, *вечер*, *ночь*; greetings *Доброе утро*, *Добрый день*, *Добрый вечер*, *Доброй ночи*).
  - Kept English where Windows or the field does: *BPM*, *Bit-perfect*, *Hi-Res Audio*, brands, symbols and example URLs (see `IDENTICAL_OK_RU`).
- **fr** (France; locale tag `fr`, label "Français", file `fr-FR.ts`; a full catalog rather than an overlay, derived from `fr.ts` and adjusted to France usage, not yet reviewed by a native speaker). **Register:** formal "vous", as fr-CA. Where it differs from fr-CA:
  - *balise* (tag, where fr-CA says *étiquette*; the Microsoft Terminology Collection renders "annotated tag" as "balise annotée" for France, a Git context, so it is not confirmed for music-file tags), *bug* (not *bogue*), *streaming* (not *diffusion en continu*), *contactez l'assistance* (not *communiquez avec*), *site web* (lower case).
  - Typography: a narrow no-break space (U+202F) before `?`, `!` and `;` and inside « », a no-break space (U+00A0) before `:`, and a narrow no-break space before `%`.
  - Keeps *chanson*, *liste de lecture*, *zone de notification* and *Paramètres*, which France and Windows France share. The same `IDENTICAL_OK` list applies.
- **en-US, en-GB:** empty until drafted.
