# pocket-tts-german-wyoming

<p align="center">
  <img src="icons/pocket-tts-hq.png" alt="pocket-tts-german-wyoming" width="320">
</p>

Natives deutsches Pocket TTS in Rust als Wyoming-TTS-Dienst für Home Assistant.
CPU-only, getestet auf Intel N100 (linux/amd64, Gracemont/AVX2). Die Docker-Runtime
enthält weder Python, PyTorch, GPU-Bibliotheken, Cargo noch einen Compiler.

- Fest gepinntes offizielles deutsches Modell, standardmäßig Q8; FP32 bleibt verfügbar.
- Echtes inkrementelles FlowLM → zustandsbehaftetes Mimi → PCM → Wyoming-Streaming.
- 24.000 Hz, Mono, vorzeichenbehaftetes PCM16 Little Endian; kein Resampling der Ausgabe.
- 27 validierte offizielle Voice-States, ausschließlich mit Locale **de-DE** angeboten.
- Juergen als Standardstimme; Stimmenwahl über Wyoming; unbekannte Stimmen führen zu einem eindeutigen Fehler.
- Native deutsche SPAN-/Prioritäts-Textnormalisierung und Streaming-Sprachgeschwindigkeitssteuerung.
- Persistente Modell-/Stimmenspeicherung, atomare validierte Downloads, separate Q8-Derivate.
- Nativer Readiness-Healthcheck, sauberes Herunterfahren, jeweils eine Synthese gleichzeitig.

Das reguläre Multi-Stage-Image und der Home-Assistant-Hörtest wurden auf dem N100 erfolgreich
bestanden. Dieses Repository stellt ein lokal baubares Docker-Projekt bereit; es wird hier nicht
behauptet, dass bereits ein Registry-Image oder ein Eintrag in Unraid Community Applications veröffentlicht ist.

## Build und Docker

```bash
docker build --platform linux/amd64 -t pocket-tts-german-wyoming:local .
```

Der Build verwendet fest Rust 1.99.0 und gepinnte Cargo-Abhängigkeiten. Die Debian-Build-Pakete
verwenden Bookworm-Repositories und keinen eingefrorenen Paket-Snapshot. Die Runtime ist `scratch`
mit den tatsächlich benötigten ELF-Bibliotheken, CA-Zertifikaten und Lizenzinformationen.
Das feste Gracemont-Target erfordert eine kompatible AVX2-CPU; dies ist kein generisches Image
für ältere x86-CPUs oder ARM.

Erstelle eigene beschreibbare Modell- und Stimmenverzeichnisse, die dem Container-Benutzer
gehören (standardmäßig UID 99 / GID 100). Ändere keine Berechtigungen anderer Projekte.

```bash
docker run -d --name pocket-tts \
  --restart unless-stopped --read-only --cap-drop ALL \
  --security-opt no-new-privileges \
  -p 10204:10204 \
  -v /absolute/path/to/dedicated/models:/app/models \
  -v /absolute/path/to/dedicated/voices:/app/voices \
  -e POCKET_TTS_SPEED=1.0 \
  pocket-tts-german-wyoming:local
```

Der Health-Status wird erst dann „ready“, wenn Modell und Standardstimme verfügbar sind und
Wyoming antwortet. Beim ersten Start können Assets heruntergeladen und Q8-Dateien erzeugt werden;
bei späteren Starts werden sie validiert und wiederverwendet. Das Originalmodell wird niemals überschrieben.
Siehe [docker/README.md](docker/README.md) und [docker/compose.yaml](docker/compose.yaml).
Das vorbereitete Unraid-Template befindet sich unter [templates/pocket-tts-german-wyoming.xml](templates/pocket-tts-german-wyoming.xml),
das öffentliche Icon unter [icons/pocket-tts.png](icons/pocket-tts.png). Der vorgesehene Image-Pfad
`ghcr.io/marco-taylor/pocket-tts-german-wyoming:latest` und die GitHub-Raw-URLs sind
nur vorbereitet; Image und CA-Eintrag sind **noch nicht veröffentlicht**.
Das Template mappt Wyoming TCP 10204 sowie eigene models-/voices-Verzeichnisse unter
`/mnt/user/appdata/pocket-tts-german-wyoming/` auf `/app/models` und `/app/voices`.
Stelle sicher, dass diese Verzeichnisse für UID 99 / GID 100 beschreibbar sind. Die Sprachgeschwindigkeit
ist von 0.8 bis 1.2 konfigurierbar, Standard ist 1.0; Zwischenwerte sind zulässig.

Füge in Home Assistant manuell **Wyoming Protocol** hinzu und verwende den Docker-Host sowie
den veröffentlichten Port (standardmäßig 10204). Wähle `pocket-tts`, Deutsch (Deutschland) und eine Stimme.
Eine mDNS-Ankündigung ist nicht implementiert. Home Assistant selbst wird von diesem Projekt
nicht konfiguriert.

## Konfiguration

[./.env.example](./.env.example) dokumentiert die Einstellungen; das Binary lädt diese Datei
nicht automatisch. Übergib die ENV-Variablen an Docker, Compose oder deinen Prozess.

| Variable | Standard | Bedeutung |
|---|---|---|
| POCKET_TTS_LANGUAGE | german | Diese Version unterstützt die gepinnte deutsche Architektur |
| POCKET_TTS_MODEL_PATH | nicht gesetzt | Explizites lokales Modellverzeichnis; deaktiviert automatisches Ersetzen/Herunterladen |
| POCKET_TTS_MODELS_DIR | /app/models | Persistentes Modell-Stammverzeichnis |
| POCKET_TTS_VOICES_DIR | /app/voices | Persistentes Stammverzeichnis für offizielle/lokale Stimmen |
| POCKET_TTS_VOICE | juergen | Konfigurierbare Standardstimme |
| POCKET_TTS_QUANTIZE | true | Q8; false wählt FP32 |
| POCKET_TTS_THREADS | 2 | Backend-Threads, zulässig 1–4; auf N100 getestet |
| POCKET_TTS_NORMALIZE | true | Deutsche Normalisierung vor dem Pocket-Tokenizer |
| POCKET_TTS_SPEED | 1.0 | Sprachgeschwindigkeit, kontinuierlicher Bereich **0.8–1.2** |
| POCKET_TTS_VOICE_CACHE_MB | 32 | Lazy-LRU-Stimmencache, zulässig 16–512 MiB |
| POCKET_TTS_DOWNLOAD_VOICES | true | Verwaltet die gepinnten öffentlichen offiziellen Stimmen automatisch |
| WYOMING_HOST | 0.0.0.0 | Listen-Interface |
| WYOMING_PORT | 10204 | TCP-Port des Containers/Prozesses |
| RUST_LOG | info | error, warn, info, debug oder trace |

Erweiterte bestehende Optionen: `POCKET_TTS_NORMALIZER=safe` ist das getestete Release-Profil.
`span`, `misaki` und `final` sind historische Vergleichsprofile und werden für den normalen
Betrieb nicht empfohlen; ihre weiter gefassten experimentellen Regeln sind unter `safe` nicht aktiv.
`POCKET_TTS_READY_FILE` schreibt optional einen diagnostischen Readiness-Snapshot; das übergeordnete
Verzeichnis muss existieren und beschreibbar sein.

### Sprachgeschwindigkeit

| Wert | Bedeutung |
|---|---|
| 0.8 | Langsamer |
| 1.0 | Normal / Referenz |
| 1.2 | Schneller |

Zwischenwerte wie 0.85 und 1.05 sind zulässig. Nicht gesetzt bedeutet 1.0.
Nicht endliche, fehlerhaft formatierte, leere oder außerhalb des Bereichs liegende Werte verursachen
einen Start-/Konfigurationsfehler: kein Clamping und kein stiller Fallback.

Native begrenzte Streaming-WSOLA-Verarbeitung folgt auf Mimi, wenn die Geschwindigkeit von 1.0 abweicht.
Die Ausgabe bleibt 24 kHz Mono PCM16 LE; die Samplerate wird nicht uminterpretiert und es wird nicht
auf eine vollständige Wellenform gewartet. Die lokale Wellenformausrichtung erhält die Tonhöhe weitgehend,
wobei Time-Stretch-Artefakte auftreten können. Bei 1.0 wird das ursprüngliche PCM unverändert durchgereicht;
die getestete Referenz war byte-identisch. ENV wird beim Start gelesen: Eine Änderung der Geschwindigkeit
erfordert das Neuerstellen des Docker-Containers mit der neuen ENV und nicht lediglich einen Neustart.

### Deutsche Normalisierung

Die vorhandenen Regeln decken Zahlen, negative Werte, Dezimalzahlen mit Komma, deutsche Tausendertrennung,
kalendergeprüfte numerische und ausgeschriebene Monatsdaten mit kontextabhängigen Ordinalformen,
Uhrzeiten, Temperaturen, Prozentangaben und gängige Home-Assistant-Einheiten ab.
Strukturierte Spans werden genau einmal verarbeitet; geschützte technische Tokens haben Vorrang.
Der Standard enthält neun eng begrenzte und getestete, von Misaki inspirierte Ergänzungen:
Euro, Cent, `z. B.`, `ca.` vor Zahlen, `bzw.`, `inkl.`, kontextabhängiges
`Nr.`, `etc.` und Monatsabkürzungen. Die Genauigkeit von Euro-/Cent-Beträgen bleibt erhalten.
Erweiterungen für `Dr.`, `Prof.`, `Fr.` und `AG` bleiben im Standardprofil ausgeschlossen.
Dies ist unser Rust-Normalizer, nicht Misaki G2P, espeak-ng oder ein Python-Dienst.

RAW-, normalisierte und Tokenizer-Strings werden derzeit zu Diagnosezwecken auf INFO geloggt.
Verwende `RUST_LOG=warn`, wenn diese potenziell privaten Texte nicht protokolliert werden sollen.
Die Erkennung frei formulierter Entitätsnamen und mehrdeutiger technischer Strings ist konservativ,
kann jedoch nicht jedes unbekannte Format schützen. Die Modellaussprache ist für beliebige Texte
nicht garantiert.

## Modelle und Stimmen

Quellen, Revisionen, Größen und SHA256 sind in [./assets.lock.json](./assets.lock.json)
und [assets/german-voices.lock.json](assets/german-voices.lock.json) fest gepinnt.
Öffentliche offizielle States benötigen kein HF_TOKEN. Im Git-Repository/Image sind keine Assets enthalten.
Downloads verwenden eigene temporäre Dateien, Validierung und atomare Umbenennung ohne Überschreiben;
Manifeste werden zuletzt geschrieben. Vorhandene ungültige Dateien werden gemeldet und niemals stillschweigend
ersetzt. Unterbrochene Teildateien werden erkannt und zur Prüfung beibehalten.

Ein explizites lokales Modellverzeichnis benötigt kompatible `config.yaml`, `tokenizer.json`
und `model.safetensors`; inkompatible oder unvollständige Daten führen ohne automatischen
Fallback zu einem Fehler. Separate Q8-Dateien benötigen ebenfalls ihre Provenienz-/Hash-Sidecar-Datei.
Q8 quantisiert die unterstützten FlowLM-Projektions-/FFN-Tensoren; Mimi und KV bleiben FP32.

Offizielle States liegen unter `/app/voices/official/german/<revision>/`.
Lokale States werden beim Start rekursiv erkannt, zum Beispiel
`/app/voices/local/my_voice.safetensors`, und als `local/my_voice` angeboten.
Sie benötigen kompatible eingebettete Metadaten oder eine benachbarte `my_voice.voice.json`:

```json
{
  "model_sha256": "<original model SHA256>",
  "model_revision": "<pinned model revision>",
  "language": "german",
  "state_format": "pocket-tts-kv-v1",
  "quantization": "fp32-kv"
}
```

KV-Formen, Offsets, Padding und endliche Werte werden validiert. Inkompatible Stimmen
werden mit Warnungen übersprungen. Eine fehlende gültige Standardstimme hält Readiness auf false.
Nur die Standardstimme wird vorgewärmt/gepinnt; andere verwenden einen begrenzten Lazy-LRU-Cache.
Dessen Budget ist kein RAM-Limit für den gesamten Prozess. WAV-Voice-Cloning ist nicht implementiert.

## Entwicklung und Tests

Rust 1.99.0, ein x86_64-Linux-C-Linker und eine Gracemont-kompatible CPU werden benötigt.

```bash
cargo fmt --all --check
cargo test --locked --workspace
cargo build --locked --release -p pocket-tts-wyoming
```

Unit-/Regressionstests benötigen keine heruntergeladenen Gewichte. Explizit ignorierte
Hardware-/TCP-Tests benötigen kompatible lokale Assets und sollten nur in einer eigenen
isolierten Umgebung ausgeführt werden. Erzeugte WAVs/Logs gehören unter ignorierte artifacts.
Output-Streaming unterscheidet sich von Input-Text-Streaming: Wyoming
`supports_synthesize_streaming=false` bezeichnet nicht unterstützte Streaming-Texteingabe.
Verbindungsabbrüche brechen die Synthese ab; gleichzeitige Anfragen erhalten `busy`.

## Danksagungen und Lizenz / Drittanbieter-Lizenzen

Vielen Dank an die Mitwirkenden von [Kyutai Pocket TTS](https://github.com/kyutai-labs/pocket-tts),
[gradium-ai XN / xn-ptts](https://github.com/gradium-ai/xn-ptts) und
[Misaki](https://github.com/hexgrad/misaki). Misaki diente als Analyse-/Referenzquelle für ausgewählte
Normalisierungsregeln, nicht als Runtime oder Phonemizer. Historisch angepasster Vergleichsquellcode
behält seine Apache-2.0-Lizenz.

Unser eigener Code: **MIT**, Copyright (c) 2026 Marco Taylor, [./LICENSE](./LICENSE).
Drittanbieter-Code und Assets behalten ihre jeweiligen Lizenzen; siehe
[./THIRD_PARTY_NOTICES.md](./THIRD_PARTY_NOTICES.md).
Insbesondere deklariert das gepinnte Kyutai-Modell-/Stimmen-Repository **CC BY 4.0**:
Modelle/Stimmen werden **nicht** unter unserer MIT-Lizenz neu lizenziert. Lokale benutzerdefinierte
Assets können andere Lizenzen besitzen. Vendored xn-ptts bleibt MIT OR Apache-2.0;
von Misaki adaptierte Dateien bleiben Apache-2.0.
