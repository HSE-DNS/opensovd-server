use std::pin::Pin;
use anyhow::anyhow;
use async_trait::async_trait;
use opensovd_core::{
    Component, Data, DataError, DataFilter, DataProvider, DiscoveryError, DiscoveryProvider,
    EntityCollection, EntityRef, Metadata,
};
use serde_json::{Value, json};
use zenoh::Session;


pub struct ZenohProvider {
    session: Session,
}

impl ZenohProvider {
    /// Initialisiert einen neuen `ZenohProvider`.
    ///
    /// # Errors
    ///
    /// Gibt einen Fehler zurück, wenn die Zenoh-Konfiguration ungültig ist oder die 
    /// Verbindung zum Router fehlschlägt.
    pub async fn new(endpoint: &str) -> anyhow::Result<Self> {
        let mut config = zenoh::Config::default();

 
        config
            .insert_json5("mode", r#""client""#)
            .map_err(|e| anyhow!("Failed to set Zenoh mode: {e}"))?;

        let endpoints_json = format!(r#"["{endpoint}"]"#);
        config
            .insert_json5("connect/endpoints", &endpoints_json)
            .map_err(|e| anyhow!("Failed to set Zenoh endpoints: {e}"))?;

        let session = zenoh::open(config)
            .await
            .map_err(|e| anyhow!("Failed to connect to Zenoh router: {e}"))?;

        tracing::info!("ZenohProvider erfolgreich mit Router verbunden");
        Ok(Self { session })
    }
}

#[async_trait]
impl DiscoveryProvider for ZenohProvider {
    async fn discover(
        &self,
    ) -> Result<
        Pin<Box<dyn futures::stream::Stream<Item = Result<(Vec<EntityRef>, EntityCollection), DiscoveryError>> + Send + 'static>>,
        DiscoveryError,
    > {
        let mut collection = EntityCollection::default();

        let data_points = vec![
            ("battery_soc".to_string(), "Battery State of Charge".to_string()),
            ("vehicle_location".to_string(), "Vehicle GPS Location".to_string()),
        ];

        tracing::info!("Zenoh-Fahrzeug mit {} Datenpunkt(en) registriert!", data_points.len());
        
        let mut component = Component::new("zenoh_car", "Zenoh Connected Car");
        let data_provider = ZenohDataProvider {
            session: self.session.clone(),
            key_prefix: "my-vehicle".to_string(),
            data_points,
        };

        component = component.with_data_provider(data_provider);
        collection.components.push(component);

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
    async fn list(&self, _filter: DataFilter) -> Result<Vec<Metadata>, DataError> {
        let items = self
            .data_points
            .iter()
            .map(|(id, name)| Metadata {
                id: id.clone(),
                name: name.clone(),
                category: "zenoh-telemetry".to_string(),
                translation_id: None,
                groups: vec![],
                tags: vec![],
                schema: None,
                is_readable: true,
                is_writable: true,
            })
            .collect();
        Ok(items)
    }

    async fn read(&self, data_id: &str, _include_schema: bool) -> Result<Data, DataError> {
        let zenoh_id = data_id.replace('_', "/");
        let key = format!("{}/{}", self.key_prefix, zenoh_id);
        
        tracing::info!("Zenoh GET: {}", key);

        let replies = self.session.get(&key)
            .await
            .map_err(|e| DataError::Internal(e.to_string()))?;

        while let Ok(reply) = replies.recv_async().await {
            match reply.result() {
                Ok(sample) => {
                    let payload = sample.payload();
                    let body = String::from_utf8_lossy(&payload.to_bytes()).into_owned();
                    tracing::info!("Daten empfangen: {}", body);

                    let data_value: Value = serde_json::from_str(&body)
                        .unwrap_or_else(|_| json!({ "raw_value": body }));

                    return Ok(Data { data: data_value, schema: None });
                },
                Err(e) => {

                    tracing::warn!("Zenoh Reply Fehler: {}", e);
                }
            }
        }

        tracing::warn!(" Keine Antwort von Zenoh für Key: {}", key);
        Err(DataError::NotFound(format!("Kein Teilnehmer für {key} gefunden")))
    }

    async fn write(&self, data_id: &str, value: Value) -> Result<(), DataError> {
        let zenoh_id = data_id.replace('_', "/");
        let key = format!("{}/{}", self.key_prefix, zenoh_id);

        tracing::info!("Zenoh PUT: {} -> {}", key, value);

        self.session.put(&key, value.to_string()).await
            .map_err(|e| DataError::Internal(e.to_string()))?;

        Ok(())
    }
}