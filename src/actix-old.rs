// use actix_web::{App, HttpRequest, HttpResponse, HttpServer, Responder, web};
// use moka::Expiry;
// use moka::sync::Cache;
// use reqwest::Client;
// use serde_json::Value;
// use std::sync::Arc;
// use std::time::{Duration, Instant};

// // In this example, we will create a `sync::Cache` with `u32` as the key, and
// // `(Expiration, String)` as the value. `Expiration` is an enum to represent the
// // expiration of the value, and `String` is the application data of the value.

// /// An enum to represent the expiration of a value.
// #[derive(Clone, Copy, Debug, Eq, PartialEq)]
// pub enum Expiration {
//     /// The value never expires.
//     Never,
//     /// The value expires after a short time. (5 seconds in this example)
//     AfterShortTime,
//     /// The value expires after a long time. (15 seconds in this example)
//     AfterLongTime,
// }

// impl Expiration {
//     /// Returns the duration of this expiration.
//     pub fn as_duration(&self) -> Option<Duration> {
//         match self {
//             Expiration::Never => None,
//             Expiration::AfterShortTime => Some(Duration::from_secs(5)),
//             Expiration::AfterLongTime => Some(Duration::from_secs(15)),
//         }
//     }
// }

// /// An expiry that implements `moka::Expiry` trait. `Expiry` trait provides the
// /// default implementations of three callback methods `expire_after_create`,
// /// `expire_after_read`, and `expire_after_update`.
// ///
// /// In this example, we only override the `expire_after_create` method.
// pub struct JsonRPCCacheExpiry;

// // pub enum JsonRPCRequestId {
// //   None,
// //   Int(u64),
// //   String(String),
// // }

// // pub struct JsonRPCResponse<T> {
// //   id: JsonRPCRequestId,
// //   jsonrpc: String,
// // }

// impl Expiry<String, (Expiration, Value)> for JsonRPCCacheExpiry {
//     /// Returns the duration of the expiration of the value that was just
//     /// created.
//     fn expire_after_create(
//         &self,
//         _key: &std::string::String,
//         value: &(Expiration, Value),
//         _current_time: Instant,
//     ) -> Option<Duration> {
//         let duration = value.0.as_duration();
//         println!(
//             "MyExpiry: expire_after_create called with key {_key} and value {value:?}. Returning {duration:?}."
//         );
//         duration
//     }
// }

// fn x() {
//     // Create a `Cache<u32, (Expiration, String)>` with an expiry `MyExpiry` and
//     // eviction listener.
//     let expiry = JsonRPCCacheExpiry;

//     let eviction_listener = |key, _value, cause| {
//         println!("Evicted key {key}. Cause: {cause:?}");
//     };

//     let cache = Cache::builder()
//         .max_capacity(100)
//         .expire_after(expiry)
//         .eviction_listener(eviction_listener)
//         .build();

//     // Insert some entries into the cache with different expirations.
//     cache.get_with(0, || (Expiration::AfterShortTime, "a".to_string()));
//     cache.get_with(1, || (Expiration::AfterLongTime, "b".to_string()));
//     cache.get_with(2, || (Expiration::Never, "c".to_string()));

//     // Verify that all the inserted entries exist.
//     assert!(cache.contains_key(&0));
//     assert!(cache.contains_key(&1));
//     assert!(cache.contains_key(&2));

//     // Sleep for 6 seconds. Key 0 should expire.
//     println!("\nSleeping for 6 seconds...\n");
//     std::thread::sleep(Duration::from_secs(6));
//     println!("Entry count: {}", cache.entry_count());

//     // Verify that key 0 has been evicted.
//     assert!(!cache.contains_key(&0));
//     assert!(cache.contains_key(&1));
//     assert!(cache.contains_key(&2));

//     // Sleep for 10 more seconds. Key 1 should expire.
//     println!("\nSleeping for 10 seconds...\n");
//     std::thread::sleep(Duration::from_secs(10));
//     println!("Entry count: {}", cache.entry_count());

//     // Verify that key 1 has been evicted.
//     assert!(!cache.contains_key(&1));
//     assert!(cache.contains_key(&2));

//     // Manually invalidate key 2.
//     cache.invalidate(&2);
//     assert!(!cache.contains_key(&2));

//     println!("\nSleeping for a second...\n");
//     std::thread::sleep(Duration::from_secs(1));
//     println!("Entry count: {}", cache.entry_count());

//     println!("\nDone!");
// }

// #[actix_web::main]
// async fn main() -> std::io::Result<()> {
//     let client = Arc::new(Client::new());

//     // Global cache object
//     let cache: Arc<Cache<String, String>> = Arc::new(Cache::builder().max_capacity(1000).build());

//     HttpServer::new(move || {
//         App::new()
//             .app_data(web::Data::from(client.clone()))
//             .app_data(web::Data::from(cache.clone()))
//             .default_service(web::to(proxy_handler))
//     })
//     .bind(("127.0.0.1", 8080))?
//     .run()
//     .await
// }

// async fn proxy_handler(
//     req: HttpRequest,
//     body: web::Bytes,
//     client: web::Data<Arc<Client>>,
//     cache: web::Data<Arc<Cache<String, String>>>,
// ) -> impl Responder {
//     let method = req.method().clone();
//     let uri = req.uri().to_string();
//     let upstream_url = format!("https://httpbin.org{}", uri); // Replace with your upstream

//     if method == actix_web::http::Method::GET {
//         if let Some(cached) = cache.get(&upstream_url) {
//             println!("🟢 Cache hit for {}", upstream_url);
//             return HttpResponse::Ok()
//                 .content_type("application/json")
//                 .body(cached);
//         }

//         println!("🔄 Cache miss for {}", upstream_url);
//         match client.get(&upstream_url).send().await {
//             Ok(resp) => {
//                 let text = resp.text().await.unwrap_or_else(|_| "{}".to_string());

//                 // Parse ?ttl=N from the query string
//                 let ttl = req
//                     .uri()
//                     .query()
//                     .and_then(|q| {
//                         q.split('&').find_map(|pair| {
//                             let mut parts = pair.splitn(2, '=');
//                             match (parts.next(), parts.next()) {
//                                 (Some("ttl"), Some(val)) => val.parse::<u64>().ok(),
//                                 _ => None,
//                             }
//                         })
//                     })
//                     .unwrap_or(60); // Default TTL: 60 seconds

//                 cache.insert(upstream_url.clone(), text.clone(), Duration::from_secs(ttl));
//                 println!("📝 Cached {} for {}s", upstream_url, ttl);

//                 HttpResponse::Ok()
//                     .content_type("application/json")
//                     .body(text)
//             }
//             Err(e) => HttpResponse::BadGateway().body(format!("Upstream error: {}", e)),
//         }
//     } else {
//         // Forward non-GET requests directly
//         println!("➡️ Forwarding {} to {}", method, upstream_url);
//         let mut req_builder = client.request(method, &upstream_url);
//         req_builder = req_builder.body(body.to_vec());

//         match req_builder.send().await {
//             Ok(resp) => {
//                 let status = resp.status();
//                 let body = resp.bytes().await.unwrap_or_default();
//                 HttpResponse::build(status).body(body)
//             }
//             Err(e) => HttpResponse::BadGateway().body(format!("Upstream error: {}", e)),
//         }
//     }
// }
