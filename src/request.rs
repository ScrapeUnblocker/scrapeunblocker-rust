//! Plumbing shared by every request builder: the query, the call, and how a
//! response body becomes the builder's output type.

use std::future::Future;
use std::pin::Pin;

use serde_json::Value;

use crate::error::{Error, Result};
use crate::models::{CapturedPage, PageHeaders, PageResult, ParsedPage};

/// The future a request builder turns into when awaited.
///
/// Boxed and `'static`, so it can be handed to `tokio::spawn`.
pub type ResponseFuture<T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'static>>;

/// Query parameters as the API names them. Setting a key again replaces it.
#[derive(Debug, Clone, Default)]
pub(crate) struct Query(Vec<(String, String)>);

impl Query {
    pub(crate) fn new() -> Self {
        Query(Vec::new())
    }

    pub(crate) fn set(&mut self, key: &str, value: impl ToString) {
        self.0.retain(|(k, _)| k != key);
        self.0.push((key.to_owned(), value.to_string()));
    }

    /// Drop a parameter, so the API's default applies.
    pub(crate) fn remove(&mut self, key: &str) {
        self.0.retain(|(k, _)| k != key);
    }

    /// A repeated parameter: one `key=value` pair per item.
    pub(crate) fn set_each<T: ToString>(&mut self, key: &str, values: impl IntoIterator<Item = T>) {
        self.0.retain(|(k, _)| k != key);
        for value in values {
            self.0.push((key.to_owned(), value.to_string()));
        }
    }

    pub(crate) fn pairs(&self) -> &[(String, String)] {
        &self.0
    }
}

/// A list sent as one string, e.g. comma-separated keywords.
pub(crate) fn joined<S: Into<String>>(values: impl IntoIterator<Item = S>, sep: &str) -> String {
    values
        .into_iter()
        .map(Into::into)
        .collect::<Vec<_>>()
        .join(sep)
}

/// One API call: an endpoint path and its query.
#[derive(Debug, Clone)]
pub(crate) struct Call {
    pub(crate) path: String,
    pub(crate) query: Query,
}

impl Call {
    pub(crate) fn new(path: impl Into<String>) -> Self {
        Call {
            path: path.into(),
            query: Query::new(),
        }
    }
}

/// How a successful response body becomes a builder's output.
pub(crate) trait Decode: Sized + Send + 'static {
    fn decode(body: Vec<u8>) -> Result<Self>;

    /// Decode a successful response, told whether it carried
    /// `X-Session-Changed: true`. Types with a `session_changed` field
    /// override this; the rest ignore the flag.
    fn decode_response(body: Vec<u8>, session_changed: bool) -> Result<Self> {
        let _ = session_changed;
        Self::decode(body)
    }
}

impl Decode for String {
    fn decode(body: Vec<u8>) -> Result<Self> {
        Ok(String::from_utf8(body)
            .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned()))
    }
}

impl Decode for Vec<u8> {
    fn decode(body: Vec<u8>) -> Result<Self> {
        Ok(body)
    }
}

impl Decode for Value {
    fn decode(body: Vec<u8>) -> Result<Self> {
        serde_json::from_slice(&body).map_err(|e| Error::Decode(format!("expected JSON: {e}")))
    }
}

impl Decode for ParsedPage {
    fn decode(body: Vec<u8>) -> Result<Self> {
        Self::decode_response(body, false)
    }

    fn decode_response(body: Vec<u8>, session_changed: bool) -> Result<Self> {
        let mut page = ParsedPage::from_value(Value::decode(body)?);
        page.session_changed = session_changed;
        Ok(page)
    }
}

impl Decode for PageResult {
    fn decode(body: Vec<u8>) -> Result<Self> {
        Self::decode_response(body, false)
    }

    fn decode_response(body: Vec<u8>, session_changed: bool) -> Result<Self> {
        let mut page = PageResult::from_value(Value::decode(body)?);
        page.session_changed = session_changed;
        Ok(page)
    }
}

impl Decode for CapturedPage {
    fn decode(body: Vec<u8>) -> Result<Self> {
        Self::decode_response(body, false)
    }

    fn decode_response(body: Vec<u8>, session_changed: bool) -> Result<Self> {
        let mut page = CapturedPage::from_value(Value::decode(body)?);
        page.session_changed = session_changed;
        Ok(page)
    }
}

impl Decode for PageHeaders {
    fn decode(body: Vec<u8>) -> Result<Self> {
        Self::decode_response(body, false)
    }

    fn decode_response(body: Vec<u8>, session_changed: bool) -> Result<Self> {
        let mut page = PageHeaders::from_value(Value::decode(body)?);
        page.session_changed = session_changed;
        Ok(page)
    }
}

/// Makes a request builder runnable: `.await` (or `.send().await`) on the
/// async client, `.send()` on the blocking one.
macro_rules! executable {
    ($ty:ident -> $out:ty) => {
        impl ::std::future::IntoFuture for $ty<$crate::Client> {
            type Output = $crate::Result<$out>;
            type IntoFuture = $crate::ResponseFuture<$out>;

            fn into_future(self) -> Self::IntoFuture {
                Box::pin(async move { self.client.execute(self.call).await })
            }
        }

        impl $ty<$crate::Client> {
            /// Send the request. The same as awaiting the builder directly.
            pub fn send(self) -> $crate::ResponseFuture<$out> {
                ::std::future::IntoFuture::into_future(self)
            }
        }

        #[cfg(feature = "blocking")]
        impl $ty<$crate::blocking::Client> {
            /// Send the request and block until the answer arrives.
            pub fn send(self) -> $crate::Result<$out> {
                self.client.execute(self.call)
            }
        }
    };
}

pub(crate) use executable;
