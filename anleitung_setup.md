# Anleitung Setup CDA und SOVD auf Raspberry Pi

## Einleitung

Die folgende Anleitung beinhaltet die Schritte, um ein testweises CDA-SOVD Setup zu erstellen. Getestet wurde das Vorgehen mit `Raspberry Pi OS lite (Trixie, 64-bit)`, auf einem Raspberryi Pi 5 mit 8 GB RAM.
Der CDA wird mithilfe des im Repository `classic-diagnostic-adapter` vorhandenen Testcontainer simuliert. Weitere Infos siehe README.md in `classic-diagnostic-adapter/testcontainer/README.md`

### Repositories
Benötigt Repositories für diesen Aufbau sind:
- HSE-DNS/opensovd-server, Branch sovd-connection-cda: https://github.com/HSE-DNS/opensovd-server/tree/sovd-connection-cda
- eclipse-opensovd/classic-diagnostic-adapter: https://github.com/eclipse-opensovd/classic-diagnostic-adapter/

### Pakete
Benötigte Pakete zum Installieren:
- Docker:
```sh
curl -fsSl https://get.docker.com | sh
sudo udermod -aG docker $USER
```
- Rust:
```sh 
curl https://sh.rustup.rs -sSf | sh
source $HOME/.cargo/env
```
- build-essential

`sudo apt install build-essential`

- Dev Headers glibc:

`sudo apt install libc6-dev`

- jq:

`sudo apt install jq` 



## Setup
Sofern die Testcontainer des CDA bereits gebaut sind, kann das Skript [starter.sh](starter.sh) dieses Repositories verwendet werden.
ACHTUNG: unter Umständen muss das Skript noch lauffähig gemacht werden mit dem Befehl: `chmod +x starter.sh`
 
Es existieren die Parameter
- `start`: startet die CDA-Testcontainer, holt sich einen Access-Token und fährt den SOVD-Server damit hoch
- `stop`: stoppt den SOVD-Server und fährt die CDA-Testcontainer herunter
- `status`: zeigt den Status der CDA-Testcontainer an

## Vorbereitung CDA
Damit der Testcontainer auf dem Raspberry Pi läuft, müssen die dazugehörigen Container für arm64 gebaut werden. Dies geht wahlweise direkt auf dem Raspberry Pi, oder auch auf x86-Geräten mit per Emulation. 
Weitere Informationen befinden sich in der README des CDA-Repositories.

a) ARM-Geräte (MAC oder Pi):
```sh
cd testcontainer
docker compose build
```
b) x86-Geräte

Für das cross compiling auf x86 müssen die QEMU-Emulatoren im WSL installiert werden. Dies geschieht durch den Befehl
```sh
docker run --privileged --rm tonistiigi/binfmt --install all
````

Danach kann der Bau mit 
```sh
DOCKER_DEFAULT_PLATFORM=linux/arm64 docker compose build
```
gestartet werden.

Wenn statt einer Linux-Umgebung die Powershell genutzt wird, ändert sich der Befehl zu 
```sh
$env:DOCKER_DEFAULT_PLATFORM="linux/arm64"; docker compose build
```

Nach dem Bau müssen die Images auf den Pi transferiert werden. Dies kann beispielsweise geschehen durch:
```sh
docker save -o pi-testcontainer.tar testcontainer-ecu-sim-arm64:latest testcontainer-cda:latest # erstellt eine Archiv mit den Images
scp pi-testcontainer.tar <username>@<IP_des_Pi>:/home/<username>/workspace/ # oder anderes beliebiges Verzeichnis
```

Auf dem Pi selber müssen diese Images dann importiert werden, hier angenommen im Verzeichnis `workspace/`:
```sh
cd /home/<username>/workspace/
docker load -i pi-testcontainer.tar
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

## Vorbereitung SOVD
Der SOVD-Server kann, sofern das starter.sh Skript nicht verwendet wird, auch von Hand gebaut werden. Dafür kann der Befehl 
```sh
cargo build -p opensovd-gateway
```
verwendet werden. Auf dem Pi kann, um die begrenzte Hardware nicht zu überlasten, das ganze wahlweise mit dem flag `--release` gebaut und/oder die Anzahl der verwendeten Kerne mit `-j 2` begrenzt werden.

Anmerkung: Wenn der Server mit dem flag `release` gebaut wird und das Starter Skript doch verwendet werden möchte, muss der Pfad im `starter.sh`-Skript zu `./target/debug/opensovd-gateway` geändert werden. 

Gestartet werden kann der Server entweder allgemein:
```sh
cargo run -p opensovd-gateway -- --url http://0.0.0.0:7690/sovd
```
oder in Verbindung mit einem Access-Token des CDA:
```sh
CDA_TOKEN="der_cda_access_token" cargo run -p opensovd-gateway -- --url http://0.0.0.0:7690/sovd
```

Beim Start des SOVD-Servers können von den default-Werten abweichende Parameter als flags mitgegeben werden:
- `--cda-host`
- `--cda-port`
- `--cda-base-path`
- `--cda-token`
- `--url`

oder, wie oben beispielsweise am `CDA_TOKEN` zu sehen, mit der Umgebungsvariable.