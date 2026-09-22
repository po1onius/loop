pub mod account;
pub mod community;
pub mod event;
pub mod messaging;
pub mod outbox;

use deadpool::managed::Object;
use diesel_async::{AsyncPgConnection, pooled_connection::AsyncDieselConnectionManager};

pub type DieselConn = Object<AsyncDieselConnectionManager<AsyncPgConnection>>;
