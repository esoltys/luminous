# Privacy Policy for Luminous Music Player / Politique de confidentialité

[English](#english) • [Français](#français)

---

## English

### Privacy Policy for Luminous Music Player

**Last Updated:** October 5, 2026

Luminous ("the Application") is designed as a local-first, privacy-focused audio player. We believe your music collection, listening habits, and personal metadata belong strictly to you.

---

#### 1. Information Collection & Use

Luminous does **not** collect, store, transmit, or monetize any personal data, usage analytics, tracking cookies, or telemetry, apart from the optional add-on requests described in section 2.

* **Local Data**: All music library indices, playlists, equalizer presets, listening history, and application preferences are stored locally on your device in an isolated database.
* **No User Accounts**: Luminous does not require user registration or account creation. If you optionally choose to connect an external account (such as MusicBrainz or ListenBrainz), authentication credentials and tokens remain strictly on your device.

---

#### 2. Network Requests & External Services

Luminous operates offline by default. Optional network requests are made strictly to deliver user-requested features:

1. **Cover Art & Metadata Lookup**: If enabled, the application queries public web APIs (such as iTunes Search API and MusicBrainz) to retrieve album artwork or track metadata for your local files.
2. **Lyric Search**: When displaying song lyrics, the application fetches lyrics from public services (such as LRCLIB and Lyrics.ovh).
3. **Application Updates**: When checking for software updates, the application queries GitHub Releases to determine if a newer version of Luminous is available.
4. **MusicBrainz & ListenBrainz Integrations**: If you connect your MusicBrainz account or configure scrobbling, Luminous only uses this connection to communicate with official MusicBrainz and ListenBrainz APIs. Your credentials and tokens remain strictly on your device, and your personal data is never tracked, collected, or shared.

5. **Add-on themes (optional, Microsoft Store version only)**: If you buy or use an add-on theme, Luminous asks the Microsoft Store whether you own it. To unlock it, Luminous sends a token issued by Microsoft that proves your purchase to Luminous's key service (luminous-keys.esoltys.dev, hosted on Cloudflare), which returns the key for that add-on, and downloads the encrypted add-on from luminous-addons.esoltys.dev. The key service does not keep a record of these requests and no account is created. As with any web request, the hosting provider can see your IP address. The key is remembered on your device for up to 30 days, protected for your Windows user account, so the service is asked about once a month. Purchases and payment are handled entirely by Microsoft under its own terms and privacy statement.

Apart from the add-on purchase token in item 5, no personal identifiers or library telemetry are included in these external requests.

---

#### 3. Data Storage & Security

Because all application data is maintained locally on your operating system, securing your data is governed by your device's security and permissions settings. Uninstalling Luminous removes local application caches according to system standards.

---

#### 4. Contact & Support

If you have questions about this privacy policy or Luminous, please open an issue on GitHub:
* **Repository**: [github.com/esoltys/luminous](https://github.com/esoltys/luminous)
* **Author**: Eric James Soltys

---

## Français

### Politique de confidentialité pour Luminous Music Player

**Dernière mise à jour :** 5 octobre 2026

Luminous (« l'Application ») est conçu comme un lecteur audio local et axé sur le respect de la vie privée. Nous estimons que votre collection musicale, vos habitudes d'écoute et vos métadonnées personnelles vous appartiennent exclusivement.

---

#### 1. Collecte et utilisation des renseignements

Luminous ne collecte, ne conserve, ne transmet et ne monétise **aucune** donnée personnelle, analyse d'utilisation, témoin de suivi (cookie) ou télémétrie, sauf les requêtes facultatives liées aux thèmes additionnels décrites à la section 2.

* **Données locales** : Tous les index de bibliothèque musicale, listes de lecture, préréglages d'égaliseur, historiques d'écoute et préférences de l'application sont stockés localement sur votre appareil dans une base de données isolée.
* **Aucun compte utilisateur** : Luminous n'exige aucune inscription ni création de compte. Si vous choisissez de connecter un compte externe (tel que MusicBrainz ou ListenBrainz), les identifiants d'authentification et les jetons demeurent strictement sur votre appareil.

---

#### 2. Requêtes réseau et services externes

Luminous fonctionne hors ligne par défaut. Les requêtes réseau facultatives sont effectuées strictement pour fournir des fonctionnalités sollicitées par l'utilisateur :

1. **Recherche de pochettes et de métadonnées** : Si activé, l'application interroge des API web publiques (telles que l'API de recherche iTunes et MusicBrainz) pour récupérer les pochettes d'album ou les métadonnées de vos fichiers locaux.
2. **Recherche de paroles** : Lors de l'affichage des paroles de chansons, l'application récupère les paroles auprès de services publics (tels que LRCLIB et Lyrics.ovh).
3. **Mises à jour de l'application** : Lors de la recherche de mises à jour logicielles, l'application interroge GitHub Releases afin de déterminer si une version plus récente de Luminous est disponible.
4. **Intégrations MusicBrainz et ListenBrainz** : Si vous connectez votre compte MusicBrainz ou configurez le scrobbling, Luminous utilise uniquement cette connexion pour communiquer avec les API officielles de MusicBrainz et ListenBrainz. Vos identifiants et jetons restent strictement sur votre appareil, et vos données personnelles ne sont jamais suivies, collectées ou partagées.

5. **Thèmes additionnels (facultatif, version Microsoft Store seulement)** : Si vous achetez ou utilisez un thème additionnel, Luminous demande au Microsoft Store si vous le possédez. Pour le déverrouiller, Luminous transmet un jeton émis par Microsoft qui prouve votre achat au service de clés de Luminous (luminous-keys.esoltys.dev, hébergé chez Cloudflare), qui renvoie la clé de ce thème, puis télécharge le thème chiffré depuis luminous-addons.esoltys.dev. Le service de clés ne conserve aucun enregistrement de ces requêtes et aucun compte n'est créé. Comme pour toute requête Web, le fournisseur d'hébergement peut voir votre adresse IP. La clé est conservée sur votre appareil pendant 30 jours au plus, protégée pour votre compte Windows, de sorte que le service est sollicité environ une fois par mois. Les achats et le paiement sont entièrement gérés par Microsoft selon ses propres conditions et sa déclaration de confidentialité.

Hormis le jeton d'achat du point 5, aucun identifiant personnel ni aucune télémétrie de bibliothèque ne sont inclus dans ces requêtes externes.

---

#### 3. Stockage et sécurité des données

Comme toutes les données de l'application sont conservées localement sur votre système d'exploitation, la sécurité de vos données est régie par les paramètres de sécurité et d'autorisations de votre appareil. La désinstallation de Luminous supprime les caches locaux de l'application conformément aux normes du système.

---

#### 4. Contact et assistance

Si vous avez des questions concernant cette politique de confidentialité ou Luminous, veuillez ouvrir un ticket sur GitHub :
* **Dépôt** : [github.com/esoltys/luminous](https://github.com/esoltys/luminous)
* **Auteur** : Eric James Soltys
