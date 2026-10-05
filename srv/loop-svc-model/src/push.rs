use crate::DieselConn;
use diesel::{
    prelude::*,
    sql_types::{BigInt, Bool, Integer, Text, Timestamptz, Uuid as SqlUuid},
};
use diesel_async::RunQueryDsl;
use uuid::Uuid;

#[derive(QueryableByName)]
pub struct PushDevice {
    #[diesel(sql_type = SqlUuid)]
    pub registration_id: Uuid,
    #[diesel(sql_type = BigInt)]
    pub user_id: i64,
    #[diesel(sql_type = Text)]
    pub token: String,
}

#[derive(QueryableByName)]
pub struct PushDelivery {
    #[diesel(sql_type = SqlUuid)]
    pub message_id: Uuid,
    #[diesel(sql_type = SqlUuid)]
    pub registration_id: Uuid,
    #[diesel(sql_type = BigInt)]
    pub user_id: i64,
    #[diesel(sql_type = Integer)]
    pub attempts: i32,
    #[diesel(sql_type = Timestamptz)]
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn register(
    installation_id: Uuid,
    user_id: i64,
    platform: &str,
    token: &str,
    conn: &mut DieselConn,
) -> QueryResult<()> {
    // A token belongs to one installation. Clearing a superseded binding also invalidates its queued deliveries.
    diesel::sql_query("DELETE FROM push_devices WHERE token=$1 AND installation_id<>$2")
        .bind::<Text, _>(token)
        .bind::<SqlUuid, _>(installation_id)
        .execute(conn)
        .await?;
    diesel::sql_query("INSERT INTO push_devices (installation_id, registration_id, user_id, platform, token) VALUES ($1,$2,$3,$4,$5) ON CONFLICT (installation_id) DO UPDATE SET registration_id=CASE WHEN push_devices.user_id<>excluded.user_id OR push_devices.token<>excluded.token OR NOT push_devices.enabled THEN excluded.registration_id ELSE push_devices.registration_id END, user_id=excluded.user_id, platform=excluded.platform, token=excluded.token, enabled=true, updated_at=now()")
        .bind::<SqlUuid,_>(installation_id).bind::<SqlUuid,_>(Uuid::now_v7()).bind::<BigInt,_>(user_id).bind::<Text,_>(platform).bind::<Text,_>(token).execute(conn).await?;
    Ok(())
}

pub async fn unregister(
    installation_id: Uuid,
    user_id: i64,
    conn: &mut DieselConn,
) -> QueryResult<()> {
    diesel::sql_query("UPDATE push_devices SET enabled=false, updated_at=now() WHERE installation_id=$1 AND user_id=$2")
        .bind::<SqlUuid,_>(installation_id).bind::<BigInt,_>(user_id).execute(conn).await?;
    Ok(())
}

pub async fn enqueue(message_id: Uuid, user_id: i64, conn: &mut DieselConn) -> QueryResult<usize> {
    diesel::sql_query("INSERT INTO push_deliveries(message_id, registration_id, user_id) SELECT $1, registration_id, user_id FROM push_devices WHERE user_id=$2 AND enabled ON CONFLICT DO NOTHING")
        .bind::<SqlUuid,_>(message_id).bind::<BigInt,_>(user_id).execute(conn).await
}

pub async fn next_delivery(conn: &mut DieselConn) -> QueryResult<Option<PushDelivery>> {
    diesel::sql_query("SELECT * FROM push_deliveries WHERE status='pending' AND next_attempt_at<=now() ORDER BY next_attempt_at LIMIT 1 FOR UPDATE SKIP LOCKED").get_result(conn).await.optional()
}

pub async fn device(
    registration_id: Uuid,
    user_id: i64,
    conn: &mut DieselConn,
) -> QueryResult<Option<PushDevice>> {
    // Lock registration through delivery; account rebinding waits until this send has completed.
    diesel::sql_query("SELECT registration_id,user_id,token FROM push_devices WHERE registration_id=$1 AND user_id=$2 AND enabled FOR UPDATE")
        .bind::<SqlUuid,_>(registration_id).bind::<BigInt,_>(user_id).get_result(conn).await.optional()
}

pub async fn eligible(
    conversation_id: Uuid,
    user_id: i64,
    seq: i64,
    conn: &mut DieselConn,
) -> QueryResult<bool> {
    #[derive(QueryableByName)]
    struct Eligible {
        #[diesel(sql_type=Bool)]
        value: bool,
    }
    let result = diesel::sql_query("SELECT EXISTS (SELECT 1 FROM conversations c WHERE c.conversation_id=$1 AND (c.kind='event_group' OR EXISTS (SELECT 1 FROM conversation_read_states s WHERE s.conversation_id=c.conversation_id AND s.user_id=$2 AND s.subscribed))) AND NOT EXISTS (SELECT 1 FROM conversation_read_states WHERE conversation_id=$1 AND user_id=$2 AND (muted OR last_read_seq >= $3)) AS value")
        .bind::<SqlUuid,_>(conversation_id).bind::<BigInt,_>(user_id).bind::<BigInt,_>(seq).get_result::<Eligible>(conn).await?;
    Ok(result.value)
}

pub async fn finish(
    delivery: &PushDelivery,
    status: &str,
    delay: i64,
    conn: &mut DieselConn,
) -> QueryResult<()> {
    diesel::sql_query("UPDATE push_deliveries SET status=$3, attempts=attempts+1, next_attempt_at=now()+make_interval(secs => $4::double precision) WHERE message_id=$1 AND registration_id=$2")
        .bind::<SqlUuid,_>(delivery.message_id).bind::<SqlUuid,_>(delivery.registration_id).bind::<Text,_>(status).bind::<BigInt,_>(delay).execute(conn).await?;
    Ok(())
}

pub async fn disable(registration_id: Uuid, conn: &mut DieselConn) -> QueryResult<()> {
    diesel::sql_query("UPDATE push_devices SET enabled=false WHERE registration_id=$1")
        .bind::<SqlUuid, _>(registration_id)
        .execute(conn)
        .await?;
    Ok(())
}

/// Keep delivery tombstones beyond the event retention window; old messages are never notified again.
pub async fn cleanup(conn: &mut DieselConn) -> QueryResult<usize> {
    diesel::sql_query("DELETE FROM push_deliveries WHERE created_at < now() - interval '30 days'")
        .execute(conn)
        .await
}
