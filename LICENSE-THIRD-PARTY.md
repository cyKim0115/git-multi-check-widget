# Third-Party Licenses

This file lists licenses for materials included in **Git Multi-Check Widget** that are
not covered by the project's own source-code license (if any).

> **Separation:** The application source code in this repository (Rust, TypeScript, HTML,
> CSS, configuration, and documentation **except** the assets listed below) is separate
> from the third-party assets documented here. Only the assets in the table below are
> governed by their respective licenses.

## Assets

| Asset | File(s) in this repo | Author | Source | License |
|-------|----------------------|--------|--------|---------|
| Git Icon | `app-icon.png`, `docs/assets/git-icon-papirus-source.png`, `src-tauri/icons/*` (generated) | Papirus Dev Team | [IconArchive — Papirus Apps Icons](https://www.iconarchive.com/show/papirus-apps-icons-by-papirus-team.html) | **GNU GPLv3** |

### Git Icon (Papirus Dev Team) — GPLv3

- **Copyright:** Papirus Dev Team (icon design); see Papirus project for details.
- **License:** GNU General Public License v3.0 (GPLv3).
- **License text:** https://www.gnu.org/licenses/gpl-3.0.html
- **Source copy in repo:** `docs/assets/git-icon-papirus-source.png` (unmodified reference PNG).
- **Use in this project:** Application window icon, installer/bundle icon (`src-tauri/icons/`, embedded in built `.exe`).

Under GPLv3, recipients of this application (when distributed with the icon embedded)
may request corresponding source for the icon; the PNG above satisfies that obligation for
the icon asset itself.

This icon license does **not** automatically apply to the rest of this repository's
source code unless you explicitly license the whole project under GPLv3.

## No other third-party assets

At the time of this notice, no other bundled visual assets require separate license
attribution beyond their npm/Rust crate licenses (see `package-lock.json`, `Cargo.lock`).
