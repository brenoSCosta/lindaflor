use redis::aio::MultiplexedConnection;

pub async fn create_client(
  valkey_url: &str,
) -> Result<MultiplexedConnection, redis::RedisError> {
  let client = redis::Client::open(valkey_url)?;
  let conn = client.get_multiplexed_async_connection().await?;
  Ok(conn)
}
