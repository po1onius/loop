pub mod account;
pub mod event;

use deadpool::managed::Object;
use diesel_async::{AsyncPgConnection, pooled_connection::AsyncDieselConnectionManager};

pub type DieselConn = Object<AsyncDieselConnectionManager<AsyncPgConnection>>;
