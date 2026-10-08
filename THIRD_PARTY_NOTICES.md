# License scope and third-party notices

Original project code is MIT licensed, Copyright (c) 2026 Marco Taylor; see
[./LICENSE](./LICENSE). This license does not replace licenses of third-party
source, dependencies, downloaded models or voice states.

## Kyutai Pocket TTS

Source reference: [kyutai-labs/pocket-tts](https://github.com/kyutai-labs/pocket-tts),
revision `41cbc84af539ea78a804ffca5f9c6edc1a22ce44`. Source license: MIT.
The unmodified upstream license is retained in
[licenses/kyutai-MIT.txt](licenses/kyutai-MIT.txt). The German configuration
fixture and runtime compatibility/text-cleanup behavior follow this revision.
This implementation is independent of the upstream Python runtime.

## XN / xn-ptts

Vendored ptts source comes from [gradium-ai/xn-ptts](https://github.com/gradium-ai/xn-ptts),
revision `83dfe119e4e1f7a13277aa4186882c758664ebf9`, with local German compatibility
patches documented in [vendor/xn-ptts/PROVENANCE.md](vendor/xn-ptts/PROVENANCE.md).
Its original MIT OR Apache-2.0 license choices remain unchanged:
[vendor/xn-ptts/LICENSE-MIT](vendor/xn-ptts/LICENSE-MIT) and
[vendor/xn-ptts/LICENSE-APACHE](vendor/xn-ptts/LICENSE-APACHE).
The tensor backend is xn 0.2.10 (MIT/Apache-2.0 in its package metadata),
obtained through Cargo rather than relicensed or vendored by this project.
Its crate provenance records commit `9c78068e9cdf8ec679f95b5a989c1b5942e1980e`
in [LaurentMazare/xn](https://github.com/LaurentMazare/xn). Unmodified license
texts from that commit are retained in licenses/xn-MIT.txt and
licenses/xn-APACHE-2.0.txt.

## Misaki-derived text normalization

Reference: [semidark/misaki German module](https://github.com/semidark/misaki/blob/bbaf917e7bf7fbbe830f74f4acda6583ce1c0caf/misaki/de.py),
revision `bbaf917e7bf7fbbe830f74f4acda6583ce1c0caf`, associated with
[hexgrad/misaki PR #97](https://github.com/hexgrad/misaki/pull/97).
Contributors identified in that revision's history: Nico Thomaier and apples-kksk.
The upstream code is Apache-2.0 licensed; the complete license is retained in
[licenses/misaki-APACHE-2.0.txt](licenses/misaki-APACHE-2.0.txt).
No upstream NOTICE file was present in the inspected pinned repository.

[crates/pocket-tts-wyoming/src/misaki_normalizer.rs](crates/pocket-tts-wyoming/src/misaki_normalizer.rs)
is a modified Rust translation of pure text-normalization code, preserving
upstream rule ordering for the historical comparison profile.
[crates/pocket-tts-wyoming/src/normalizer_extensions.rs](crates/pocket-tts-wyoming/src/normalizer_extensions.rs)
contains the historical selective adaptation. These files retain Apache-2.0
notices and are exceptions to the project's own-code MIT license. Neither
profile is the default release normalizer.

The default `safe` profile is our native SPAN/Priority normalizer with nine
conservative rule groups informed by the Misaki analysis: Euro, Cent,
`z. B.`, `ca.`, `bzw.`, `inkl.`, context-dependent `Nr.`, `etc.` and calendar
month abbreviations. It does not copy Misaki's regex chain or depend on Misaki,
Python, espeak-ng, G2P, accent stripping or phoneme overrides at runtime.
Excluded title/company expansions remain excluded in the default profile.

## Models, tokenizers and voices

Downloaded assets come from Kyutai's
[pocket-tts-without-voice-cloning repository](https://huggingface.co/kyutai/pocket-tts-without-voice-cloning).
Its [model card at the pinned model/voice revision](https://huggingface.co/kyutai/pocket-tts-without-voice-cloning/blob/1e08e6a23401048648a9fdcfde2f89348215c2a7/README.md)
declares [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/).
Tokenizer revision and asset checksums are separately pinned in ./assets.lock.json;
voice checksums are in assets/german-voices.lock.json. Kyutai is credited as
the asset provider. Original FP32/BF16 assets are preserved; Q8 is a locally
derived, modified representation with recorded provenance.

Model, tokenizer and voice files are not included in Git or the Docker image.
Their applicable upstream license remains separate from our MIT license.
Manually installed models/voices may have different licenses; the installer
must retain their provenance and applicable permissions. No blanket MIT license
or voice ownership claim is made by this project.

## Other dependencies and runtime libraries

Cargo dependencies retain their own licenses declared by their pinned packages
in ./Cargo.lock. A compiled executable is not a relicensing of those components.
The Docker runtime copies Debian's ELF dependency closure and certificate bundle;
these retain their respective upstream licenses. Distribution of an image must
preserve the applicable library notices/source availability obligations as well
as the project and adaptation notices. Debian copyright records for copied
runtime packages are included in the runtime image.

The runtime also includes a Cargo dependency/license inventory and available
unmodified upstream license/notice files under `/usr/share/licenses/cargo/`.
Registry source-package code is not included there. Source for the unmodified
Debian runtime libraries is available through the Debian source archives for
the recorded package/source versions (https://sources.debian.org/ and
https://snapshot.debian.org/). These binaries retain their upstream LGPL/GPL
with applicable runtime exceptions rather than the project MIT license.
