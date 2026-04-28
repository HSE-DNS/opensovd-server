use anyhow::anyhow;
use async_trait::async_trait;
use opensovd_core::{
    Component, Data, DataError, DataFilter, DataProvider, DiscoveryError, DiscoveryProvider,
    EntityCollection, EntityRef, Metadata,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::pin::Pin;
use std::time::Duration;
use tokio::time::timeout;
use zenoh::Session;

/// ZENTRALE KONFIGURATION
/// Diese Struktur wird in der main.rs verwendet, um den Provider einzustellen.
pub struct ZenohConfig {
    /// Die Netzwerkadresse des Zenoh Routers (z.B. "tcp/127.0.0.1:7447")
    pub endpoint: String,
    /// Der Selector für die Discovery (Standard: "**" für alles)
    pub discovery_selector: String,
    /// Welcher Teil des Pfades ist der Robot-Name? (0 = erster Teil)
    pub robot_name_index: usize,
}

impl Default for ZenohConfig {
    fn default() -> Self {
        Self {
            endpoint: "tcp/localhost:7447".to_string(),
            discovery_selector: "**".to_string(),
            robot_name_index: 0,
        }
    }
}

pub struct ZenohProvider {
    session: Session,
    config: ZenohConfig,
}

impl ZenohProvider {
    /// Erstellt einen neuen ZenohProvider und baut die Verbindung zum Router auf.
    pub async fn new(config: ZenohConfig) -> anyhow::Result<Self> {
        let mut zenoh_config = zenoh::Config::default();
        
        // Modus als Client festlegen
        zenoh_config.insert_json5("mode", r#""client""#).map_err(|e| anyhow!("{e}"))?;
        
        // Endpunkte konfigurieren
        let endpoints_json = format!(r#"["{}"]"#, config.endpoint);
        zenoh_config.insert_json5("connect/endpoints", &endpoints_json).map_err(|e| anyhow!("{e}"))?;

        // Session öffnen
        let session = zenoh::open(zenoh_config).await.map_err(|e| anyhow!("{e}"))?;
        tracing::info!("ZenohProvider erfolgreich verbunden mit {}", config.endpoint);
        
        Ok(Self { session, config })
    }
}

#[async_trait]
impl DiscoveryProvider for ZenohProvider {
    /// Discovery via LIVELINESS TOKENS.
    /// Voraussetzung: Das Zenoh-Team (Roboter) muss liveliness().declare_token() nutzen.
    async fn discover(
        &self,
    ) -> Result<
        Pin<Box<dyn futures::stream::Stream<Item = Result<(Vec<EntityRef>, EntityCollection), DiscoveryError>> + Send + 'static>>,
        DiscoveryError,
    > {
        let mut collection = EntityCollection::default();
        
        tracing::info!("Starte Discovery via Zenoh Liveliness Tokens mit Selector: {}", self.config.discovery_selector);

        // Abfrage aller aktiven Liveliness Tokens im Netzwerk
        let replies = self.session.liveliness().get(&self.config.discovery_selector)
            .await
            .map_err(|e| DiscoveryError::Other(e.to_string().into()))?;

        let mut robot_map: HashMap<String, Vec<(String, String)>> = HashMap::new();

        // Alle gefundenen Tokens verarbeiten
        while let Ok(reply) = replies.recv_async().await {
            if let Ok(sample) = reply.result() {
                let key = sample.key_expr().as_str();
                let parts: Vec<&str> = key.split('/').collect();
                
                // Extraktion des Roboternamens basierend auf dem konfigurierten Index
                if let Some(robot_name) = parts.get(self.config.robot_name_index) {
                    let robot_name_str = robot_name.to_string();
                    
                    // Der Rest des Pfades wird zur SOVD Data ID (verbunden mit Unterstrichen)
                    let data_parts: Vec<&str> = parts.iter()
                        .enumerate()
                        .filter(|&(i, _)| i != self.config.robot_name_index)
                        .map(|(_, &s)| s)
                        .collect();
                    
                    let data_id = data_parts.join("_");
                    
                    if !data_id.is_empty() {
                        robot_map.entry(robot_name_str)
                            .or_default()
                            .push((data_id.clone(), data_id));
                    }
                }
            }
        }

        // Komponenten für die SOVD-Topology erstellen
        for (robot_name, points) in robot_map {
            tracing::info!("SOVD Komponente erstellt: '{}' (via Liveliness Token)", robot_name);
            let mut component = Component::new(robot_name.clone(), format!("Zenoh Robot {robot_name}"));
            
            component = component.with_data_provider(ZenohDataProvider {
                session: self.session.clone(),
                key_prefix: robot_name,
                data_points: points,
            });
            collection.components.push(component);
        }

        let stream = futures::stream::once(std::future::ready(Ok((vec![], collection))));
        Ok(Box::pin(stream))
    }
}


pub struct ZenohDataProvider {
    session: Session,
    key_prefix: String,
    data_points: Vec<(String, String)>,
}

#[async_trait]
impl DataProvider for ZenohDataProvider {
    /// Listet alle verfügbaren Datenpunkte dieser Komponente auf.
    async fn list(&self, _filter: DataFilter) -> Result<Vec<Metadata>, DataError> {
        Ok(self.data_points.iter().map(|(id, name)| Metadata {
            id: id.clone(),
            name: name.clone(),
            category: "zenoh-pubsub".to_string(),
            is_readable: true,
            is_writable: true,
            translation_id: None,
            groups: vec![],
            tags: vec![],
            schema: None,
        }).collect())
    }

    /// Liest Daten via SUBSCRIBER (Pull-from-Push Prinzip).
    /// Da der Roboter kein Queryable hat, warten wir hier auf die nächste Publikation.
    async fn read(&self, data_id: &str, _include_schema: bool) -> Result<Data, DataError> {
        let zenoh_id = data_id.replace('_', "/");
        let key = format!("{}/{}", self.key_prefix, zenoh_id);
        
        // Wir abonnieren den Key kurzzeitig, um den nächsten Push-Wert zu fangen
        let subscriber = self.session.declare_subscriber(&key)
            .await
            .map_err(|e| DataError::Internal(e.to_string()))?;

        tracing::debug!("SOVD Read: Warte auf Publikation für {}", key);

        // Timeout von 5 Sekunden: Wenn der Roboter nichts sendet, brechen wir ab
        match timeout(Duration::from_secs(5), subscriber.recv_async()).await {
            Ok(Ok(sample)) => {
                let body = String::from_utf8_lossy(&sample.payload().to_bytes()).into_owned();
                // Versuchen als JSON zu parsen, sonst in raw_value einbetten
                let data: Value = serde_json::from_str(&body).unwrap_or_else(|_| json!({"raw_value": body}));
                Ok(Data { data, schema: None })
            }
            Ok(Err(e)) => Err(DataError::Internal(format!("Subscriber Fehler: {}", e))),
            Err(_) => Err(DataError::NotFound(format!("Timeout: Roboter hat auf '{}' keine Daten gesendet", key))),
        }
    }

    /// Schreibt Daten via PUBLISHER (session.put).
    async fn write(&self, data_id: &str, value: Value) -> Result<(), DataError> {
        let zenoh_id = data_id.replace('_', "/");
        let key = format!("{}/{}", self.key_prefix, zenoh_id);
        
        // In Zenoh ist ein Put ein klassischer Publish
        self.session.put(&key, value.to_string())
            .await
            .map_err(|e| DataError::Internal(e.to_string()))?;
        
        tracing::info!("SOVD Write: Wert auf Zenoh-Key '{}' publiziert", key);
        Ok(())
    }
}