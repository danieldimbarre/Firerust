//! A very simple library to implement the Firebase real-time database in your code with the best performance
//! 
//! # Instalation
//! Add this to your `Cargo.toml`:
//! ```toml
//! [dependencies]
//! firerust = { version = "1.0.0" }
//! ```
//! 
//! # Examples
//! A basic example of data fetch:
//! ```rust
//! use firerust::FirebaseClient;
//! use serde_json::Value;
//! use std::error::Error;
//!
//! fn main() -> Result<(), Box<dyn Error>> {
//!     let client = FirebaseClient::new("https://docs-examples.firebaseio.com/")?;
//!     let reference = client.reference("/");
//! 
//!     reference.set(serde_json::json!({
//!         "message": "Hello, world!",
//!     }))?;
//!     println!("{:?}", reference.get::<Value>());
//! 
//!     Ok(())
//! }
//! ```


use connector::{ Connector, Method, EventStream , EventType };
use std::fmt::{ Display, Formatter };
use serde::de::DeserializeOwned;
use std::sync::{ Arc, Mutex };
use std::thread::JoinHandle;
use std::error::Error;
use serde_json::Value;
use serde::Serialize;
use std::io::Read;
use url::Url;


/// TLS Connector for Firebase client
pub mod connector;


/// Connects and authenticates client to Firebase
#[derive(Debug, Clone)]
pub struct FirebaseClient {
    connector: Connector,
    api_key: Option<String>,
}

impl FirebaseClient {

    /// Creates a new instance of FirebaseClient with the given url
    /// and connects to the Firebase server
    /// 
    /// # Example
    /// ```rust
    /// use firerust::FirebaseClient;
    /// 
    /// let client = FirebaseClient::new("https://docs-examples.firebaseio.com/")?;
    /// ```
    /// 
    /// # Errors
    /// Returns an error if the url is invalid or the connection to the server fails
    pub fn new(url: impl ToString) -> Result<FirebaseClient, Box<dyn Error>> {
        let url = Url::parse(&url.to_string())?;

        let domain = match url.domain() {
            Some(domain) => {
                if !domain.contains(".firebaseio.com") && !domain.contains(".firebasedatabase.app") {
                    return Err(Box::new(FirebaseError::new("Invalid domain")));
                }

                domain.to_string()
            },
            None => return Err(Box::new(FirebaseError::new("Invalid domain")))
        };

        let port = match url.port_or_known_default() {
            Some(port) => port,
            None => 443 as u16
        };


        Ok(FirebaseClient {
            api_key: None,
            connector: Connector::new(domain, port)?
        })
    }

    /// Sets the API key for the client
    /// 
    /// # Example
    /// ```rust
    /// use firerust::FirebaseClient;
    /// 
    /// let client = FirebaseClient::new("https://docs-examples.firebaseio.com/")?;
    /// client.auth("ID_TOKEN");
    /// ```
    pub fn auth(&mut self, api_key: impl ToString) {
        self.api_key = Some(api_key.to_string());
    }

    /// Creates a new reference to the given path
    /// 
    /// # Example
    /// ```rust
    /// use firerust::FirebaseClient;
    /// 
    /// let client = FirebaseClient::new("https://docs-examples.firebaseio.com/")?;
    /// let reference = client.reference("/");
    /// ```
    pub fn reference(&self, path: impl ToString) -> RealtimeReference {
        RealtimeReference::new(self, path.to_string())
    }
}


/// A reference to a Firebase real-time database
pub struct RealtimeReference {
    client: FirebaseClient,
    path: String,
}

impl RealtimeReference {

    /// Creates a new instance of RealtimeReference with the given path
    pub fn new(client: &FirebaseClient, path: impl ToString) -> RealtimeReference {
        RealtimeReference {
            client: client.clone(),
            path: path.to_string(),
        }
    }

    /// Set reference from the child path
    /// 
    /// # Example
    /// ```rust
    /// use firerust::FirebaseClient;
    /// 
    /// let client = FirebaseClient::new("https://docs-examples.firebaseio.com/")?;
    /// let reference = client.reference("/");
    /// let child_reference = reference.child("child");
    /// ```
    pub fn child(&self, path: impl ToString) -> RealtimeReference {
        RealtimeReference::new(&self.client, format!("{}/{}", self.path, path.to_string()))
    }

    /// Get the value of the reference
    /// 
    /// # Example
    /// ```rust
    /// use firerust::FirebaseClient;
    /// use serde_json::Value;
    /// 
    /// let client = FirebaseClient::new("https://docs-examples.firebaseio.com/")?;
    /// assert_eq!(client.reference("/").get::<Value>().is_ok(), true);
    /// ```
    /// 
    /// # Errors
    /// Returns an error if the value is not a valid Response
    pub fn get<T>(&self) -> Result<T, Box<dyn Error>> where T: Serialize + DeserializeOwned {
        let response = self.client.connector.request(Method::Get, self.path.clone(), match self.client.api_key {
            Some(ref api_key) => Some(format!("?auth={}", api_key)),
            None => None
        }, None)?;

        if response.status().code() != 200 {
            return Err(Box::new(FirebaseError::new(format!("{} {}", response.status().code(), response.status().message()))));
        }

        Ok(serde_json::from_str(response.body())?)
    }

    /// Set the value of the reference
    /// 
    /// # Example
    /// ```rust
    /// use firerust::FirebaseClient;
    /// 
    /// let client = FirebaseClient::new("https://docs-examples.firebaseio.com/")?;
    /// client.reference("/").set(serde_json::json!({
    ///    "message": "Hello, world!",
    /// }))?;
    /// ```
    pub fn set<T>(&self, data: T) -> Result<(), Box<dyn Error>>  where T: Serialize {
        let data = serde_json::to_string(&data)?;

        let response = self.client.connector.request(Method::Put, self.path.clone(), Some(match self.client.api_key {
            Some(ref api_key) => format!("?print=silent&auth={}", api_key),
            None => "?print=silent".to_string()
        }), Some(data))?;

        if response.status().code() != 204 {
            return Err(Box::new(FirebaseError::new(format!("{} {}", response.status().code(), response.status().message()))));
        }

        Ok(())
    }

    /// Set a unique child value of the reference
    /// 
    /// # Example
    /// ```rust
    /// use firerust::FirebaseClient;
    /// 
    /// let client = FirebaseClient::new("https://docs-examples.firebaseio.com/")?;
    /// client.reference("/posts").set_unique(serde_json::json!({
    ///     "message": "Hello, world!",
    /// }))?;
    /// ```
    pub fn set_unique<T>(&self, data: T) -> Result<(), Box<dyn Error>>  where T: Serialize {
        let data = serde_json::to_string(&data)?;

        let response = self.client.connector.request(Method::Post, self.path.clone(), Some(match self.client.api_key {
            Some(ref api_key) => format!("?print=silent&auth={}", api_key),
            None => "?print=silent".to_string()
        }), Some(data))?;

        if response.status().code() != 204 {
            return Err(Box::new(FirebaseError::new(format!("{} {}", response.status().code(), response.status().message()))));
        }

        Ok(())
    }

    /// Update the value of the reference
    /// 
    /// # Example
    /// ```rust
    /// use firerust::FirebaseClient;
    /// 
    /// let client = FirebaseClient::new("https://docs-examples.firebaseio.com/")?;
    /// client.reference("/").update(serde_json::json!({
    ///     "message": "New hello, world!",
    /// }))?;
    /// ```
    pub fn update<T>(&self, data: T) -> Result<(), Box<dyn Error>> where T: Serialize {
        let data = serde_json::to_string(&data)?;

        let response = self.client.connector.request(Method::Patch, self.path.clone(), Some(match self.client.api_key {
            Some(ref api_key) => format!("?print=silent&auth={}", api_key),
            None => "?print=silent".to_string()
        }), Some(data))?;

        if response.status().code() != 204 {
            return Err(Box::new(FirebaseError::new(format!("{} {}", response.status().code(), response.status().message()))));
        }

        Ok(())
    }

    /// Delete the value of the reference
    /// 
    /// # Example
    /// ```rust
    /// use firerust::FirebaseClient;
    /// 
    /// let client = FirebaseClient::new("https://docs-examples.firebaseio.com/")?;
    /// client.reference("/").delete()?;
    /// ```
    pub fn delete(&self) -> Result<(), Box<dyn Error>> {
        let response = self.client.connector.request(Method::Delete, self.path.clone(), Some(match self.client.api_key {
            Some(ref api_key) => format!("?print=silent&auth={}", api_key),
            None => "?print=silent".to_string()
        }), None)?;

        if response.status().code() != 204 {
            return Err(Box::new(FirebaseError::new(format!("{} {}", response.status().code(), response.status().message()))));
        }

        Ok(())
    }

    /// Get the value of the reference as a stream
    /// 
    /// # Example
    /// ```rust
    /// use firerust::FirebaseClient;
    /// use serde_json::Value;
    /// 
    /// let client = FirebaseClient::new("https://docs-examples.firebaseio.com/")?;
    /// client.reference("/").on_snapshot(|snapshot: Value| {
    ///     assert_eq!(snapshot["message"].as_str(), Some("Hello, world!"));
    ///     Ok(())
    /// });
    pub fn on_snapshot<T, F>(&self, callback: F) -> Result<JoinHandle<()>, Box<dyn Error>> where 
        T: Send + 'static,
        F: Send + Copy + 'static,
        T: Serialize + DeserializeOwned,
        F: FnOnce(T) -> Result<(), Box<dyn Error>>
    {
        let (status, event_stream, mut stream) = self.client.connector.event_stream(self.path.clone(), match self.client.api_key {
            Some(ref api_key) => format!("?auth={}", api_key),
            None => "".to_string()
        })?;

        if status.code() != 200 {
            return Err(Box::new(FirebaseError::new(format!("{} {}", status.code(), status.message()))));
        }

        let data = serde_json::from_str::<Value>(event_stream.data())?;

        let snap = match data.get("data") {
            Some(snap) => Arc::new(Mutex::new(snap.clone())),
            None => return Err(Box::new(FirebaseError::new("Invalid data")))
        };

        match snap.clone().lock() {
            Ok(snap) => {
                let data = serde_json::from_value::<T>(snap.clone())?;
                callback(data)?;
            },
            Err(_) => return Err(Box::new(FirebaseError::new("Invalid data")))
        };

        Ok(std::thread::spawn(move || loop {
            let mut data = Vec::new();

            loop {
                let mut buf = [0; 1024];
                let len = match stream.read(&mut buf) {
                    Ok(len) => len,
                    Err(_) => break
                };

                data.extend_from_slice(&buf[..len]);

                if len < 1024 {
                    break;
                }
            }

            let event_stream = match String::from_utf8(data) {
                Ok(event_stream) => match EventStream::try_from(event_stream) {
                    Ok(event_stream) => event_stream,
                    Err(_) => continue
                },
                Err(_) => continue
            };

            let data = match serde_json::from_str::<Value>(event_stream.data()) {
                Ok(data) => data,
                Err(_) => continue
            };

            let path = match data["path"].as_str() {
                Some(path) => match path {
                    "/" => "",
                    _ => path
                },
                None => continue
            };

            let snapshot =  match data.get("data") {
                Some(snap) => snap.clone(),
                None => continue
            };

            match event_stream.event() {
                EventType::Put => {
                    let mut snap = match snap.lock() {
                        Ok(snap) => snap,
                        Err(_) => continue
                    };

                    let pointer = match snap.pointer_mut(&path) {
                        Some(pointer) => pointer,
                        None => continue
                    };

                    *pointer = snapshot;

                    let data = match serde_json::from_value::<T>(snap.clone()) {
                        Ok(data) => data,
                        Err(_) => continue,
                    };

                    match callback(data) {
                        Ok(_) => {},
                        Err(_) => {}
                    };
                },
                EventType::Patch => {
                    let mut snap = match snap.lock() {
                        Ok(snap) => snap,
                        Err(_) => continue
                    };

                    let pointer = match snap.pointer_mut(&path) {
                        Some(pointer) => pointer,
                        None => continue
                    };

                    match RealtimeReference::merge_value(pointer, snapshot) {
                        Ok(_) => {},
                        Err(_) => continue
                    };

                    let data = match serde_json::from_value::<T>(snap.clone()) {
                        Ok(data) => data,
                        Err(_) => continue
                    };

                    match callback(data) {
                        Ok(_) => {},
                        Err(_) => {}
                    };
                },                
                EventType::Cancel => return,
                EventType::AuthRevoked => return,
                EventType::KeepAlive => continue,
            };
        }))
    }

    #[doc(hidden)]
    pub fn merge_value(a: &mut Value, b: Value) -> Result<(), Box<dyn Error>> {
        match (a.clone(), b.clone()) {
            (Value::Object(mut a), Value::Object(b)) => {
                for (k, v) in b {
                    if v.is_null() {
                        a.remove(&k);
                    } else {
                        RealtimeReference::merge_value(a.entry(k).or_insert(Value::Null), v)?;
                    }
                }
    
                return Ok(());
            }
            _ => {
                *a = b;
            }
        };

        Ok(())
    }
}


/// Firebase client error
#[derive(Debug)]
struct FirebaseError {
    message: String
}

impl FirebaseError {
    fn new(message: impl ToString) -> FirebaseError {
        FirebaseError {
            message: message.to_string()
        }
    }
}

impl Error for FirebaseError {
    fn description(&self) -> &str {
        &self.message
    }
}

impl Display for FirebaseError {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}