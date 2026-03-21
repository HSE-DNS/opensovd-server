use opensovd_core::Component;
use opensovd_core::DiscoveryProvider;

pub struct CdaProvider {
    host: String,
    port: u16,
    base_path: String,
    token: String,
}

impl CdaProvider {
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        let token = std::env::var("CDA_TOKEN").unwrap_or_default();
        let base_path =
            std::env::var("CDA_BASE_PATH").unwrap_or_else(|_| "/vehicle/v15".to_string());
        Self {
            host: host.into(),
            port,
            base_path,
            token,
        }
    }

    /// Asynchronously fetches data from the CDA via REST/HTTP.
    ///
    /// # Errors
    ///
    /// Returns a `reqwest::Error` if the HTTP request fails or the response body cannot be read.
    pub async fn fetch_cda_path(
        &self,
        path: &str,
    ) -> Result<(reqwest::StatusCode, String), reqwest::Error> {
        let url = format!("http://{}:{}{}", self.host, self.port, path);

        let client = reqwest::Client::new();
        let response = client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .send()
            .await?;

        let status = response.status();
        let body = response.text().await?;
        Ok((status, body))
    }
}

/// Data provider for CDA components.
pub struct CdaDataProvider {
    component_id: String,
    cda_host: String,
    cda_port: u16,
    base_path: String,
    token: String,
    /// Stores (ID, Name) tuples of available data points.
    data_points: Vec<(String, String)>,
}

#[async_trait::async_trait]
impl opensovd_core::DataProvider for CdaDataProvider {
    async fn list(
        &self,
        _filter: opensovd_core::DataFilter,
    ) -> Result<Vec<opensovd_core::Metadata>, opensovd_core::DataError> {
        let items = self
            .data_points
            .iter()
            .map(|(id, name)| opensovd_core::Metadata {
                id: id.clone(),
                name: name.clone(),
                category: "cda-data".to_string(),
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

    async fn read(
        &self,
        data_id: &str,
        _include_schema: bool,
    ) -> Result<opensovd_core::Data, opensovd_core::DataError> {
        let url = format!(
            "http://{}:{}{}/components/{}/data/{}",
            self.cda_host, self.cda_port, self.base_path, self.component_id, data_id
        );

        let client = reqwest::Client::new();
        let response = match client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .send()
            .await
        {
            Ok(res) => res,
            Err(e) => {
                return Ok(opensovd_core::Data {
                    data: serde_json::json!({ "error": format!("CDA nicht erreichbar: {}", e) }),
                    schema: None,
                });
            }
        };

        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        if !status.is_success() {
            let error_details =
                serde_json::from_str::<serde_json::Value>(&body).unwrap_or(serde_json::json!(body));
            return Ok(opensovd_core::Data {
                data: serde_json::json!({
                    "error": format!("CDA lieferte HTTP Status: {}", status),
                    "cda_message": error_details
                }),
                schema: None,
            });
        }

        let mut data_value = serde_json::from_str::<serde_json::Value>(&body)
            .unwrap_or(serde_json::json!({ "raw": body }));

        // Unwrap nested "data" field if present (CDA specific behavior).
        if let Some(inner_data) = data_value.get("data") {
            data_value = inner_data.clone();
        }

        Ok(opensovd_core::Data {
            data: data_value,
            schema: None,
        })
    }

    async fn write(
        &self,
        data_id: &str,
        value: serde_json::Value,
    ) -> Result<(), opensovd_core::DataError> {
        let url = format!(
            "http://{}:{}{}/components/{}/data/{}",
            self.cda_host, self.cda_port, self.base_path, self.component_id, data_id
        );

        let client = reqwest::Client::new();

        // Wrap payload in a "data" object required by the CDA.
        let payload = serde_json::json!({
            "data": value
        });

        let response = match client
            .put(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Content-Type", "application/json")
            .body(payload.to_string())
            .send()
            .await
        {
            Ok(res) => res,
            Err(e) => {
                return Err(opensovd_core::DataError::Internal(format!(
                    "CDA nicht erreichbar: {e}"
                )));
            }
        };

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(opensovd_core::DataError::Internal(format!(
                "CDA HTTP Status {status}: {body}"
            )));
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl DiscoveryProvider for CdaProvider {
    #[allow(clippy::too_many_lines)]
    async fn discover(
        &self,
    ) -> Result<
        std::pin::Pin<
            std::boxed::Box<
                dyn futures::stream::Stream<
                        Item = Result<
                            (
                                Vec<opensovd_core::EntityRef>,
                                opensovd_core::EntityCollection,
                            ),
                            opensovd_core::DiscoveryError,
                        >,
                    > + Send
                    + 'static,
            >,
        >,
        opensovd_core::DiscoveryError,
    > {
        let mut collection = opensovd_core::EntityCollection::default();

        let components_path = format!("{}/components", self.base_path);
        match self.fetch_cda_path(&components_path).await {
            Ok((status, text)) => {
                tracing::info!(
                    "Received REST response! Status: {status}, Length: {}",
                    text.len()
                );

                if text.is_empty() {
                    tracing::warn!("Response body is empty. Check the URL or CDA status.");
                } else {
                    match serde_json::from_str::<serde_json::Value>(&text) {
                        Ok(json) => {
                            tracing::info!("Successfully parsed JSON from CDA:\n{json:#?}");

                            if let Some(items) = json.get("items").and_then(|v| v.as_array()) {
                                if items.is_empty() {
                                    tracing::info!(
                                        "Info: The 'items' list from CDA is currently empty."
                                    );
                                } else {
                                    for (index, item) in items.iter().enumerate() {
                                        tracing::info!("- Processing item {index}: {item}");

                                        let id = item
                                            .get("id")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("unknown_id");
                                        let name = item
                                            .get("name")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("Unknown Component");

                                        let mut cda_component = Component::new(id, name);
                                        let mut found_data = Vec::new();

                                        // Fetch available data points for the component.
                                        let data_path =
                                            format!("{}/components/{id}/data", self.base_path);
                                        tracing::info!("  -> Fetching data from: {data_path}");
                                        match self.fetch_cda_path(&data_path).await {
                                            Ok((data_status, data_text)) => {
                                                if data_status.is_success() && !data_text.is_empty()
                                                {
                                                    if let Ok(data_json) =
                                                        serde_json::from_str::<serde_json::Value>(
                                                            &data_text,
                                                        )
                                                    {
                                                        if let Some(data_items) = data_json
                                                            .get("items")
                                                            .and_then(|v| v.as_array())
                                                        {
                                                            for data_item in data_items {
                                                                let data_id = data_item
                                                                    .get("id")
                                                                    .and_then(|v| v.as_str())
                                                                    .unwrap_or("unknown_data");
                                                                let data_name = data_item
                                                                    .get("name")
                                                                    .and_then(|v| v.as_str())
                                                                    .unwrap_or("Unknown Data");

                                                                found_data.push((
                                                                    data_id.to_string(),
                                                                    data_name.to_string(),
                                                                ));
                                                                tracing::info!(
                                                                    "     Discovered data point: {data_id} - {data_name}"
                                                                );
                                                            }
                                                        }
                                                    } else {
                                                        tracing::warn!(
                                                            "     Failed to parse JSON data for {id}."
                                                        );
                                                    }
                                                } else {
                                                    tracing::info!(
                                                        "    Info: No data found for component {id} (Status: {data_status})"
                                                    );
                                                }
                                            }
                                            Err(e) => {
                                                tracing::error!(
                                                    "     ERROR fetching data for {id}: {e}"
                                                );
                                            }
                                        }

                                        let provider = CdaDataProvider {
                                            component_id: id.to_string(),
                                            cda_host: self.host.clone(),
                                            cda_port: self.port,
                                            base_path: self.base_path.clone(),
                                            token: self.token.clone(),
                                            data_points: found_data,
                                        };
                                        cda_component = cda_component.with_data_provider(provider);

                                        collection.components.push(cda_component);
                                    }
                                }
                            }
                        }
                        Err(e) => tracing::warn!("Failed to parse JSON from CDA: {e}"),
                    }
                }
            }
            Err(e) => tracing::error!("ERROR during REST connection: {e}"),
        }

        let stream = futures::stream::once(std::future::ready(Ok((vec![], collection))));
        Ok(Box::pin(stream))
    }
}
