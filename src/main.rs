use warp::Filter;

#[tokio::main]
async fn main() {
    let db = models::blank_db();

    let routes = filters::init().or(filters::todos(db)).recover(handlers::rejection);

    warp::serve(routes)
        .run(([127, 0, 0, 1], 3030))
        .await;
}

mod filters {
    use super::handlers;
    use warp::{Reply, Filter, Rejection};
    use std::net::SocketAddr;
    use warp::http::header::{HeaderMap, HeaderValue};
    use serde::de::DeserializeOwned;
    use super::models::{DB, ListOptions, Employee, Todo};

    pub fn init() -> impl Filter<Extract=impl Reply, Error=Rejection> + Clone {
        readme()
            .or(hello())
            .or(hi())
            .or(sleep())
            .or(register())
    }

    /// curl http://127.0.0.1:3030
    pub fn readme() -> impl Filter<Extract=impl Reply, Error=Rejection> + Clone {
        warp::get()
            .and(warp::path::end())
            .and(warp::fs::file("./README.md"))
    }

    /// curl http://127.0.0.1:3030/hello/m1
    pub fn hello() -> impl Filter<Extract=impl Reply, Error=Rejection> + Clone {
        let mut headers = HeaderMap::new();
        headers.insert("foo", HeaderValue::from_static("bar"));

        warp::path("hello").and(warp::get()).and(warp::path::param())
            .and(warp::header::<SocketAddr>("host"))
            .and(warp::header::<String>("user-agent"))
            .and_then(handlers::hello).with(warp::reply::with::headers(headers))
    }

    /// curl http://127.0.0.1:3030/hi
    pub fn hi() -> impl Filter<Extract=impl Reply, Error=Rejection> + Clone {
        warp::path("hi").and(warp::get()).and_then(handlers::hi)
    }

    /// curl http://127.0.0.1:3030/sleep/:second
    pub fn sleep() -> impl Filter<Extract=impl Reply, Error=Rejection> + Clone {
        warp::path("sleep").and(warp::get()).and(warp::path::param()).and_then(handlers::sleepy)
    }

    /// curl -d '{"name":"Sean","rate":2}' -H "Content-Type: application/json" -X POST http://127.0.0.1:3030/register
    pub fn register() -> impl Filter<Extract=impl Reply, Error=Rejection> + Clone {
        warp::path("register").and(warp::post()).and(json_body::<Employee>()).and_then(handlers::register)
    }

    /// The 4 TODOs filters combined.
    pub fn todos(
        db: DB,
    ) -> impl Filter<Extract=impl Reply, Error=Rejection> + Clone {
        todos_list(db.clone())
            .or(todos_create(db.clone()))
            .or(todos_update(db.clone()))
            .or(todos_delete(db))
    }

    /// curl "http://127.0.0.1:3030/todos?offset=3&limit=5"
    pub fn todos_list(
        db: DB,
    ) -> impl Filter<Extract=impl Reply, Error=Rejection> + Clone {
        warp::path!("todos")
            .and(warp::get())
            .and(warp::query::<ListOptions>())
            .and(with_db(db))
            .and_then(handlers::list_todos)
    }

    /// curl -d '{"text":"Sean","id":2,"completed":false}' -H "Content-Type: application/json" -X POST http://127.0.0.1:3030/todos
    pub fn todos_create(
        db: DB,
    ) -> impl Filter<Extract=impl Reply, Error=Rejection> + Clone {
        warp::path!("todos")
            .and(warp::post())
            .and(json_body::<Todo>())
            .and(with_db(db))
            .and_then(handlers::create_todo)
    }

    /// curl -d '{"text":"Sean","id":2,"completed":true}' -H "Content-Type: application/json" -X PUT http://127.0.0.1:3030/todos/2
    pub fn todos_update(
        db: DB,
    ) -> impl Filter<Extract=impl Reply, Error=Rejection> + Clone {
        warp::path!("todos" / u64)
            .and(warp::put())
            .and(json_body::<Todo>())
            .and(with_db(db))
            .and_then(handlers::update_todo)
    }

    /// curl -H "Authorization: Bearer admin" -X DELETE http://127.0.0.1:3030/todos/2
    pub fn todos_delete(
        db: DB,
    ) -> impl Filter<Extract=impl Reply, Error=Rejection> + Clone {
        // We'll make one of our endpoints admin-only to show how authentication filters are used
        let admin_only = warp::header::exact("authorization", "Bearer admin");

        warp::path!("todos" / u64)
            // It is important to put the auth check _after_ the path filters.
            // If we put the auth check before, the request `PUT /todos/invalid-string`
            // would try this filter and reject because the authorization header doesn't match,
            // rather because the param is wrong for that other path.
            .and(admin_only)
            .and(warp::delete())
            .and(with_db(db))
            .and_then(handlers::delete_todo)
    }

    fn with_db(db: DB) -> impl Filter<Extract=(DB, ), Error=std::convert::Infallible> + Clone {
        warp::any().map(move || db.clone())
    }

    fn json_body<T: DeserializeOwned + Send>() -> impl Filter<Extract=(T, ), Error=warp::Rejection> + Clone {
        // When accepting a body, we want a JSON body
        // (and to reject huge payloads)...
        warp::body::content_length_limit(1024 * 16).and(warp::body::json())
    }
}

mod handlers {
    use warp::http::StatusCode;
    use warp::{Rejection, Reply};
    use std::convert::Infallible;
    use std::net::SocketAddr;
    use std::time::Duration;

    use super::models::{NormalReply, Employee, Seconds, ErrorMessage, DB, ListOptions, Todo};

    pub async fn hi() -> Result<impl Reply, Infallible> {
        Ok("Hello, World!")
    }

    pub async fn hello(name: String, host: SocketAddr, agent: String) -> Result<impl Reply, Infallible> {
        let result = NormalReply {
            name,
            host,
            agent,
        };

        Ok(warp::reply::json(&result))
    }

    pub async fn register(employee: Employee) -> Result<impl Reply, Infallible> {
        Ok(warp::reply::json(&employee))
    }

    pub async fn sleepy(Seconds(seconds): Seconds) -> Result<impl Reply, Infallible> {
        tokio::time::sleep(Duration::from_secs(seconds)).await;
        Ok(format!("I waited {} seconds!", seconds))
    }

    pub async fn rejection(err: Rejection) -> Result<impl Reply, Infallible> {
        let code;
        let message;

        if err.is_not_found() {
            code = StatusCode::NOT_FOUND;
            message = "NOT_FOUND";
        } else if let Some(_) = err.find::<warp::filters::body::BodyDeserializeError>() {
            // This error happens if the body could not be deserialized correctly
            // We can use the cause to analyze the error and customize the error message
            code = StatusCode::BAD_REQUEST;
            message = "BAD_REQUEST";
        } else if let Some(_) = err.find::<warp::reject::MethodNotAllowed>() {
            // We can handle a specific error, here METHOD_NOT_ALLOWED,
            // and render it however we want
            code = StatusCode::METHOD_NOT_ALLOWED;
            message = "METHOD_NOT_ALLOWED";
        } else {
            // We should have expected this... Just log and say its a 500
            eprintln!("unhandled rejection: {:?}", err);
            code = StatusCode::INTERNAL_SERVER_ERROR;
            message = "UNHANDLED_REJECTION";
        }

        let json = warp::reply::json(&ErrorMessage {
            code: code.as_u16(),
            message: message.into(),
        });

        Ok(warp::reply::with_status(json, code))
    }

    pub async fn list_todos(opts: ListOptions, db: DB) -> Result<impl Reply, Infallible> {
        // Just return a JSON array of todos, applying the limit and offset.
        let todos = db.lock().await;
        let todos: Vec<Todo> = todos
            .clone()
            .into_iter()
            .skip(opts.offset.unwrap_or(0))
            .take(opts.limit.unwrap_or(usize::MAX))
            .collect();
        Ok(warp::reply::json(&todos))
    }

    pub async fn create_todo(create: Todo, db: DB) -> Result<impl Reply, Infallible> {
        let mut vec = db.lock().await;

        for todo in vec.iter() {
            if todo.id == create.id {
                // Todo with id already exists, return `400 BadRequest`.
                return Ok(StatusCode::BAD_REQUEST);
            }
        }

        // No existing Todo with id, so insert and return `201 Created`.
        vec.push(create);

        Ok(StatusCode::CREATED)
    }

    pub async fn update_todo(
        id: u64,
        update: Todo,
        db: DB,
    ) -> Result<impl Reply, Infallible> {
        let mut vec = db.lock().await;

        // Look for the specified Todo...
        for todo in vec.iter_mut() {
            if todo.id == id {
                *todo = update;
                return Ok(StatusCode::OK);
            }
        }

        // If the for loop didn't return OK, then the ID doesn't exist...
        Ok(StatusCode::NOT_FOUND)
    }

    pub async fn delete_todo(id: u64, db: DB) -> Result<impl Reply, Infallible> {
        let mut vec = db.lock().await;

        let len = vec.len();
        vec.retain(|todo| {
            // Retain all Todos that aren't this id...
            // In other words, remove all that *are* this id...
            todo.id != id
        });

        // If the vec is smaller, we found and deleted a Todo!
        let deleted = vec.len() != len;

        if deleted {
            // respond with a `204 No Content`, which means successful,
            // yet no body expected...
            Ok(StatusCode::NO_CONTENT)
        } else {
            Ok(StatusCode::NOT_FOUND)
        }
    }
}

mod models {
    use serde_derive::{Deserialize, Serialize};
    use std::str::FromStr;
    use std::net::SocketAddr;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    #[derive(Serialize)]
    pub struct NormalReply {
        pub name: String,
        pub host: SocketAddr,
        pub agent: String,
    }

    #[derive(Serialize)]
    pub struct ErrorMessage {
        pub code: u16,
        pub message: String,
    }

    pub struct Seconds(pub u64);

    impl FromStr for Seconds {
        type Err = ();
        fn from_str(src: &str) -> Result<Self, Self::Err> {
            src.parse::<u64>().map_err(|_| ()).and_then(|num| {
                if num <= 5 {
                    Ok(Seconds(num))
                } else {
                    Err(())
                }
            })
        }
    }

    #[derive(Deserialize, Serialize)]
    pub struct Employee {
        pub name: String,
        pub rate: u32,
    }

    pub type DB = Arc<Mutex<Vec<Todo>>>;

    pub fn blank_db() -> DB {
        Arc::new(Mutex::new(Vec::new()))
    }

    #[derive(Debug, Deserialize, Serialize, Clone)]
    pub struct Todo {
        pub id: u64,
        pub text: String,
        pub completed: bool,
    }

    // The query parameters for list_todos.
    #[derive(Debug, Deserialize)]
    pub struct ListOptions {
        pub offset: Option<usize>,
        pub limit: Option<usize>,
    }
}

#[cfg(test)]
mod tests {
    use warp::http::StatusCode;
    use warp::test::request;

    use super::{
        filters,
        models::{self, Todo},
    };

    #[tokio::test]
    async fn test_hi_ok() {
        let api = filters::hi();

        let resp = request()
            .method("GET")
            .path("/hi")
            .reply(&api)
            .await;

        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_hi_not_found() {
        let api = filters::hi();

        let resp = request()
            .method("GET")
            .path("/ho")
            .reply(&api)
            .await;

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_post() {
        let db = models::blank_db();
        let api = filters::todos(db);

        let resp = request()
            .method("POST")
            .path("/todos")
            .json(&todo1())
            .reply(&api)
            .await;

        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn test_post_conflict() {
        let db = models::blank_db();
        db.lock().await.push(todo1());
        let api = filters::todos(db);

        let resp = request()
            .method("POST")
            .path("/todos")
            .json(&todo1())
            .reply(&api)
            .await;

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    fn todo1() -> Todo {
        Todo {
            id: 1,
            text: "test 1".into(),
            completed: false,
        }
    }
}