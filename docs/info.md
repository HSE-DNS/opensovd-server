# OpenSOVD Gateway & CDA Integration – Zusammenfassung

## 1. Was haben wir gebaut?

Wir haben ein in Rust geschriebenes **OpenSOVD-Gateway** erweitert, sodass es als Vermittler (Middleware) zwischen der genormten SOVD-Schnittstelle und deinem CDA-Server agiert. Der CDA nutzt wiederum ODX-Dateien, um über UDS/DoIP mit den eigentlichen Steuergeräten zu kommunizieren.

## 2. Die Kernkomponenten im Code (`cda.rs`)

### A) DiscoveryProvider (`CdaProvider`)

* **Aufgabe:** Dynamisches Erkennen der Fahrzeug-Topologie (Welche Steuergeräte gibt es?).
* **Ablauf:** Beim Start macht das Gateway einen REST-Aufruf an den CDA (`/vehicle/v15/components`), um Komponenten wie `flxc1000` zu finden.
* **Datenpunkte:** Für jede gefundene Komponente ruft der Provider im zweiten Schritt sofort deren verfügbare Diagnosedaten ab (`/vehicle/v15/components/.../data`) und merkt sich diese (z.B. `VINDataIdentifier`).
* Aus diesen Infos wird die Komponente für OpenSOVD im Speicher gebaut und der *DataProvider* daran angehängt.

### B) DataProvider (`CdaDataProvider`)

* **Aufgabe:** Beantworten von konkreten Lese- und Schreibanfragen (REST-Calls der Endnutzer).
* **`list()` Methode:** Gibt die beim Start gemerkten Datenpunkte strukturiert aus, wenn jemand die Liste der Komponente anfragt.
* **`read()` Methode:** Das Herzstück! Wenn jemand z.B. die VIN anfragt, baut diese Methode dynamisch die Ziel-URL zusammen, hängt das Security-Token an, fragt live den CDA nach dem aktuellen Steuergeräte-Wert ab und reicht das Ergebnis (oder eine schöne Fehlermeldung) direkt als JSON weiter.
* **`write()` Methode:** Ist momentan absichtlich noch nicht implementiert (wirft einen `ReadOnly` Fehler).

## 3. Dynamik & Sicherheit

* **Komplette Flexibilität:** Im Rust-Code ist kein einziger Datenpunkt hartcodiert. Du kannst **jeden** Datenpunkt abfragen, der beim Starten des Servers im Terminal aufgelistet wurde (vorausgesetzt die ODX-Datei kennt den UDS-Befehl dafür).
* **Konfiguration & Sicherheit:** Das JWT-Token wird nun *ausschließlich* aus der Umgebungsvariable `CDA_TOKEN` geladen (es gibt keine unsicheren Fallback-Tokens mehr im Quellcode!). Auch der API-Pfad des CDA (vorher hartcodiert) lässt sich nun bei Bedarf flexibel über die Variable `CDA_BASE_PATH` anpassen.

## 4. Wichtige Befehle für das nächste Mal

**1. Gateway starten:**
Normaler Start:

```bash
cargo run -p opensovd-gateway -- --url http://0.0.0.0:7690/sovd
```

Start mit einem bestimmten Token:

```bash
CDA_TOKEN="dein_neues_token" cargo run -p opensovd-gateway -- --url http://0.0.0.0:7690/sovd
```

**2. Datenliste einer Komponente abfragen:**

```bash
curl http://127.0.0.1:7690/sovd/v1/components/flxc1000/data
```

**3. Live-Datenpunkt lesen (z.B. VIN oder Identification):**

```bash
curl http://127.0.0.1:7690/sovd/v1/components/flxc1000/data/VINDataIdentifier
```

```bash
curl http://127.0.0.1:7690/sovd/v1/components/flxc1000/data/Identification
```
