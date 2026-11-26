---
paths:
  - "web/**"
---

# Web UI

`web/README.md` is the reference for this app; read it before the first change to the UI in a session. The points below are the ones that break the build or a project rule when missed.

- Svelte 5 with runes and TypeScript. `package.json` has `devDependencies` only and the page loads nothing from the network (no CDN, no remote fonts, no analytics). A new dependency needs a reason stated in the pull request.
- `src/lib/api/bindings/` and `src/lib/api/contract.ts` are generated. Change `crates/kanjo-core/src/api.rs` and run `scripts/gen-bindings.sh`; never edit them by hand.
- Every visible string lives in `src/lib/i18n` with an entry in both `en.ts` and `pl.ts`. Numbers, dates and money go through `fmt`.
- `Money` requires its `confidence` prop, and every chart sits in a `ChartCard` with its table view.
- Colours, spacing and type sizes come from the tokens in `src/lib/styles/tokens.css`, never from raw values. Status colours mean state only and come with an icon and a word.
- The server sends a strict Content-Security-Policy. A static `style="..."` attribute, `{@html}` and style or script elements created at run time are blocked, and `src/csp.test.ts` fails on them.
- New views in `src/lib/mock` aggregate from the one synthetic request list, so figures agree across endpoints.
- Lay out for 360 px first, keep keyboard use complete and focus visible.
- Gate, run in `web/`: `npm run check`, `npm test` and `npm run build`, with zero warnings.
