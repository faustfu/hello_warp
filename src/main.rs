use warp::{Filter};
use serde::ser::{Serialize, SerializeStruct};
use serde::Serializer;
use warp::http::header::{HeaderMap, HeaderValue};

#[tokio::main]
async fn main() {
    let mut headers = HeaderMap::new();
    headers.insert("server", HeaderValue::from_static("wee/0"));
    headers.insert("foo", HeaderValue::from_static("bar"));

    // GET /hello/warp => 200 OK with body "Hello, warp!"
    let hello = warp::path!("hello" / String)
        .and(warp::header::<String>("user-agent"))
        .map(|name, agent| {
            let result = NormalReply {
                name,
                agent,
            };

            warp::reply::json(&result)
        }).with(warp::reply::with::headers(headers));

    warp::serve(hello)
        .run(([127, 0, 0, 1], 3030))
        .await;
}

struct NormalReply {
    name: String,
    agent: String,
}

impl Serialize for NormalReply {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
    {
        let mut s = serializer.serialize_struct("NormalReply", 3)?;
        s.serialize_field("name", &self.name)?;
        s.serialize_field("agent", &self.agent)?;
        s.end()
    }
}

