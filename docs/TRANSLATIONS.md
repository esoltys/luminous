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

- **it:** pending the draft-versus-contribution diff from #1421 (Claude drafts blind, then diffs against the contributed file). Seed this list from that diff: terminology differences, register, over-literal calques, key coverage.
- **es, de, uk, ru, en-US, en-GB, fr (France):** empty until drafted.
