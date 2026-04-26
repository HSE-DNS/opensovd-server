> 🇩🇪 **Deutsch** | 🇬🇧 [Read in English](hackfest_instruction.en.md)

# Setup-Anleitung: CDA und SOVD auf dem Raspberry Pi

## Inhaltsverzeichnis
- [Einleitung](#einleitung)
  - [Repositories](#repositories)
  - [Pakete](#pakete)
- [Setup mittels Starter-Skript](#setup-mittels-starter-skript)
- [Vorbereitung des CDA](#vorbereitung-des-cda)
- [Vorbereitung des SOVD-Servers](#vorbereitung-des-sovd-servers)
- [Flashen des S32K148 Boards](#flashen-des-s32k148-boards)

## Einleitung

Die folgende Anleitung beinhaltet die Schritte, um ein testweises CDA-SOVD-Setup zu erstellen. Getestet wurde das Vorgehen mit `Raspberry Pi OS lite (Debian GNU/Linux 13.04 "Trixie", 64-bit)` auf einem Raspberry Pi 5 mit 8 GB RAM.
Der CDA wird mithilfe des im unten verlinkten Repository `classic-diagnostic-adapter` vorhandenen Testcontainers simuliert. Weitere Infos siehe README.md in `classic-diagnostic-adapter/testcontainer/README.md`

### Repositories
Benötigte Repositories für diesen Aufbau sind:
- HSE-DNS/opensovd-server, Branch sovd-connection-cda: https://github.com/HSE-DNS/opensovd-server/tree/sovd-connection-cda
- eclipse-opensovd/classic-diagnostic-adapter: https://github.com/eclipse-opensovd/classic-diagnostic-adapter/

### Pakete

Mit den folgenden Befehlen können die zusätzlich benötigten Pakete installiert werden:
- Docker:
```sh
curl -fsSl https://get.docker.com | sh
```
```sh
sudo usermod -aG docker $USER
```
Anmerkung: Danach bitte einmal Ab- und Anmelden, damit die Änderung wirksam wird.
- Rust:
```sh 
curl https://sh.rustup.rs -sSf | sh
```
```sh
source $HOME/.cargo/env
```

- jq:
```sh
sudo apt install jq
```

Getestet wurde der Aufbau mit den folgenden Versionen:

| Paket      | Version        |
|------------|----------------|
| Docker     | 29.4.1         |
| Rust       | 1.94.0-nightly |
| jq         | 1.7            |


## Setup mittels Starter-Skript
Sofern die Testcontainer des CDA bereits gebaut sind, kann das Skript [starter.sh](../starter.sh) dieses Repositories verwendet werden.
ACHTUNG: Unter Umständen muss das Skript noch ausführbar gemacht werden mit dem Befehl: `chmod +x starter.sh`
 
Es existieren folgende Parameter, mit denen das Skript gestartet werden kann:

- `start`: startet die CDA-Testcontainer, holt sich einen Access-Token und fährt den SOVD-Server damit hoch
- `stop`: stoppt den SOVD-Server und fährt die CDA-Testcontainer herunter
- `status`: zeigt den Status der CDA-Testcontainer an

## Vorbereitung des CDA

Info: Eine detaillierte Beschreibung für den CDA und den CDA-Provider findet sich im Verzeichnis [docs/](../docs/). 

Damit der Testcontainer auf dem Raspberry Pi läuft, müssen die dazugehörigen Container für arm64 gebaut werden. Dies geht wahlweise direkt auf dem Raspberry Pi oder auch auf x86-Geräten per Emulation.

Weitere Informationen befinden sich in der README des CDA-Repositories.

a) ARM-Geräte (MAC oder Pi):

Ausgehend vom Root-Verzeichnis des CDA Repositories werden die Images wie folgt gebaut:
```sh
cd testcontainer && docker compose build
```

b) x86-Geräte

Für das Cross-Compiling auf x86 müssen möglicherweise noch die QEMU-Emulatoren installiert werden. Dies geschieht durch den Befehl
```sh
docker run --privileged --rm tonistiigi/binfmt --install all
```
Zusätzliche Informationen dazu: https://docs.docker.com/build/building/multi-platform/#install-qemu-manually

Danach kann der Bau im `testcontainer`-Verzeichnis mit 
```sh
DOCKER_DEFAULT_PLATFORM=linux/arm64 docker compose build
```
gestartet werden.
Anmerkung: Der in der oben verlinkten Docker-Doku genutzte Befehl `docker buildx build` lässt sich zwar für den Bau **eines** Image anwenden, **nicht** aber für einen `docker compose`-Command.

Wenn statt einer Linux-Umgebung die PowerShell genutzt wird, ändert sich der Befehl zu
```sh
$env:DOCKER_DEFAULT_PLATFORM="linux/arm64"; docker compose build
```

Nach dem Bau müssen die Images auf den Pi transferiert werden. Dies kann beispielsweise geschehen durch:
```sh
docker save -o pi-testcontainer.tar testcontainer-ecu-sim-arm64:latest testcontainer-cda:latest # erstellt ein Archiv mit den Images
```
```sh
scp pi-testcontainer.tar <username>@<IP_des_Pi>:/home/<username>/workspace/ # oder anderes beliebiges Verzeichnis
```

Auf dem Pi selbst müssen diese Images dann importiert werden, hier angenommen im Verzeichnis `workspace/`:
```sh
cd /home/<username>/workspace/
```
```sh
docker load -i pi-testcontainer.tar
```

Anmerkung: Falls die bereitgestellten oder gebauten Images nicht die Default-Namen `testcontainer-ecu-sim:latest` und `testcontainer-cda:latest` tragen, muss in der `docker-compose.yml` des CDA-Verzeichnisses `testcontainer` beim jeweiligen Job der Image-Name angegeben werden, da sonst die Images nicht gefunden und folglich neu gebaut werden, was einige Zeit dauern kann. 

Das folgende Beispiel geht davon aus, dass die Images `ecu-sim-arm64:latest` und `cda-arm64:latest` heißen. Da sie nicht dem Default-Naming entsprechen, muss 
```yaml
services:
  ecu-sim:
    build:
      context: ./ecu-sim
      dockerfile: docker/Dockerfile
	(...)
  cda:
    build:
      context: ..
      dockerfile: testcontainer/cda/Dockerfile
```
geändert werden zu:
```yaml
services:
  ecu-sim:
    image: ecu-sim-arm64:latest # NEW
    build:
      context: ./ecu-sim
      dockerfile: docker/Dockerfile
	(...)
  cda:
    image: cda-arm64:latest # NEW
    build:
      context: ..
      dockerfile: testcontainer/cda/Dockerfile
```


Nachdem die Images gebaut bzw. geladen sind, kann im Verzeichnis `testcontainer` des CDA-Repositories das Netzwerk gestartet werden, mit dem Befehl
```sh
docker compose up -d
```
Zum Herunterfahren den Befehl
```sh
docker compose down
```
verwenden.

## Vorbereitung des SOVD-Servers
Der SOVD-Server kann, sofern das starter.sh-Skript nicht verwendet wird, auch von Hand gebaut werden. Dafür kann der Befehl
```sh
cargo build -p opensovd-gateway
```
verwendet werden. Auf dem Pi kann, falls nicht genügend Ressourcen zur Verfügung stehen, das Ganze wahlweise mit dem Flag `--release` gebaut und/oder die Anzahl der verwendeten Kerne mit `-j 2` begrenzt werden.

Anmerkung: Wenn der Server mit dem Flag `--release` gebaut wird und das Starter-Skript doch verwendet werden möchte, muss der Pfad in Zeile 34 des Skripts `starter.sh` zu `./target/release/opensovd-gateway` geändert werden. 

Gestartet werden kann der Server entweder allgemein:
```sh
cargo run -p opensovd-gateway -- --url http://0.0.0.0:7690/sovd
```
oder in Verbindung mit einem Access-Token des CDA:
```sh
CDA_TOKEN="der_cda_access_token" cargo run -p opensovd-gateway -- --url http://0.0.0.0:7690/sovd
```

Beim Start des SOVD-Servers können von den default-Werten abweichende Parameter als Flags mitgegeben werden:
- `--cda-host`
- `--cda-port`
- `--cda-base-path`
- `--cda-token`
- `--url`

oder, wie oben beispielsweise am `CDA_TOKEN` zu sehen, über eine Umgebungsvariable. 

Weitere Infos sind, wie bereits erwähnt, in [cda.md](cda.md) und [cdaProvider.md](cdaProvider.md) zu finden. 

## Flashen des S32K148 Boards
Um das S32K148 Board zu Flashen, bitte den Anweisungen in der verlinkten Dokumentation folgen: https://eclipse-openbsw.github.io/openbsw/sphinx_docs/doc/dev/learning/setup/index.html

NOTE: 
- Die Dokumentation bezieht sich auf Ubuntu 22.04, das Vorgehen wurde jedoch auch mit Ubuntu 24.04 LTS getestet, es traten keine Probleme auf.
- Die Dokumentation verwendet die Befehle 
    ```sh
    cmake --preset s32k148-gcc
    cmake --build --preset s32k148-gcc
    ```
    für das Bauen mit CMake für das Board. Jedoch existiert dieses Target nicht mehr.
    
    Stattdessen sollten für Boards auf dem SDV HackFest folgende Befehle verwendet werden:
    ```sh
    cmake --preset s32k148-freertos-gcc
    ```
    ```sh 
    cmake --build --preset s32k148-freertos-gcc
    ```
