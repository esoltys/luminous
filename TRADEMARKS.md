# Luminous Trademark Policy

This policy governs the use of the **Luminous** name, iconography, and visual branding. While the Luminous source code is open source under the [MIT License](LICENSE), the Luminous brand and trademarks are **not** licensed under the MIT License.

This policy exists to safeguard community trust, prevent user confusion about which builds are official, and prevent unauthorized commercial exploitation of the Luminous brand.

---

## 1. The Marks

"Marks" refers to all trademarks, service marks, trade names, logos, icons, and visual branding owned by Eric James Soltys for Luminous, including without limitation:

* **Word marks:** "Luminous", "Luminous Music Player", `LuminousMusicPlayer`, and the official application identifiers (`org.luminous.music` on Linux/desktop and `39231EricJamesSoltys.LuminousMusicPlayer` on Microsoft Store / Windows).
* **Visual marks & iconography:** `app-icon.png`, `app-icon.svg`, `static/app-icon.svg`, `static/favicon.png`, `static/luminous-mark.svg`, `static/signature.svg`, all icon assets under `src-tauri/icons/`, and official store or promotional artwork.

As noted in [NOTICE](NOTICE), official iconography and visual branding assets are excluded from the MIT License and are All Rights Reserved.

---

## 2. Permitted Uses (No Permission Required)

You do **not** need written permission to:

* **Build for personal or internal use:** Download, compile, and run the source code on your own devices or within your organization.
* **Redistribute unmodified source code:** Mirror or redistribute the source repository under the terms of the [MIT License](LICENSE), provided all license and [NOTICE](NOTICE) files remain intact.
* **Nominative fair use:** Truthfully reference "Luminous" in plain text to describe compatibility, origin, or commentary—for example:
  * *"A fork of Luminous Music Player"*
  * *"Built from Luminous source code"*
  * *"Plugin/theme compatible with Luminous"*

  Such references must not imply official endorsement, sponsorship, or affiliation.

---

## 3. Prohibited Uses

Without prior written permission from the trademark owner, you may **not**:

* **Republish built artifacts under official branding:** Compile, package, or distribute binaries, installers, or app packages on commercial app stores (including the Microsoft Store, Apple App Store, Google Play, or similar marketplaces) or public distribution channels using the Luminous name, iconography, or visual branding.
* **Imply official status:** Use the Marks (or any confusingly similar name, icon, or domain) in a way that suggests your build, fork, product, or service is an official Luminous release or is endorsed by the Luminous project.
* **Monetize the brand:** Sell, monetize, or bundle builds or services under the Luminous name or iconography.

---

## 4. Rebranding Requirements for Third-Party Builds & Forks

If you distribute compiled binaries, a modified version, or a fork publicly or commercially, you **must** rebrand it before distribution:

1. **Change app identity:** Update `productName`, `mainBinaryName`, and `identifier` in `src-tauri/tauri.conf.json` and `src-tauri/tauri.windows.conf.json` (along with corresponding package/manifest metadata in `package.json`, `Cargo.toml`, `flatpak/`, etc.).
2. **Replace all official iconography and branding:** Remove and replace `app-icon.png`, `app-icon.svg`, `static/app-icon.svg`, `static/favicon.png`, `static/luminous-mark.svg`, `static/signature.svg`, and the assets under `src-tauri/icons/`.
3. **Preserve attribution:** Retain the MIT copyright notice in [LICENSE](LICENSE) and the exclusions in [NOTICE](NOTICE) as required by the license.

---

## 5. Requests & Questions

If you would like to request permission for a specific distribution (such as an official community package) or have questions about this policy, please open an issue on the [Luminous GitHub repository](https://github.com/esoltys/luminous/issues) or contact the project maintainer directly.
