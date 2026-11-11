# uBlock Origin scriptlets

`ubo-scriptlets.json` holds the scriptlets of
[uBlock Origin](https://github.com/gorhill/uBlock) (`src/js/resources/`),
converted to the resource format of Brave's adblock-rust
(`Engine::use_resources`). Filter rules such as
`www.youtube.com##+js(json-prune, ...)` in uBO's filter lists run these
scriptlets in the page at document start. Ferrite uses them to block YouTube
ads the way uBlock Origin and Brave do.

- Source: gorhill/uBlock commit `2bc679d99afe5bddd8fe3ab60137966a12123f41`
  (2026-11-10).
- Licence: **GPL-3.0** (`LICENSE-uBlock-Origin.txt`), Copyright (C) Raymond Hill
  and contributors. The file is data that Ferrite (MIT) loads at runtime; it is
  shipped as a separate file and is not compiled into the binary. Its source
  is the uBlock Origin repository at the commit above.
- Regenerate: `scripts/update-adblock-resources.sh`. It clones uBlock and runs
  `scripts/build-adblock-resources.py`, which reads the sources statically
  (none of uBlock's code is executed).

Update it when uBO's filter lists start using scriptlets this file does not
have yet (the lists update daily, this file only with Ferrite).
