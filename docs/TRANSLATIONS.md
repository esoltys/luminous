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
- Counts: `{count}` is substituted verbatim. Where English has a singular/plural pair, it is two keys, `fooOne` ("1 song added") and `fooMany` ("{count} songs added"), and the caller picks `One` only when the count is exactly 1. There is no CLDR plural-category support, so a language with more than two forms (Russian, Ukrainian, Czech, Polish) cannot inflect a noun after a number correctly. Until plural support lands, phrase those strings so the noun does not agree with the number ("Songs: {count}"), and flag any string where that is impossible.
- Keep numbers, dates and durations out of the string when the UI already formats them with `formatNumber` or `Intl`; the token receives the locale-formatted value.

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

Link status when this was written: the hosts above that have a URL answered, except the RAE, Gramota.ru and Merriam-Webster sites, which refuse automated requests and need a manual check in a browser. No stable FranceTerme or Ukrainian Orthography URL has been confirmed yet; find and add them when the fr (France) and uk locales are drafted. Treat every authority as "proposed" until the person drafting that language has used it once and confirmed it gives usable answers for UI terms.

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
- **es, de, uk, ru, en-US, en-GB, fr (France):** empty until drafted.
