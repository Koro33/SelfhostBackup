use crate::config;
use opendal::{layers::TracingLayer, services::S3, Operator};

pub async fn init_s3(s3: &config::S3) -> Result<Operator, opendal::Error> {
    let mut builder = S3::default();
    builder
        .root(&s3.root)
        .bucket(&s3.bucket)
        .region(&s3.region)
        .endpoint(&s3.endpoint)
        .access_key_id(&s3.access_key_id)
        .secret_access_key(&s3.secret_access_key);
    let op = Operator::new(builder)?.layer(TracingLayer).finish();

    tracing::debug!("{:?}", op.info());

    Ok(op)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::read_config;

    #[tokio::test]
    async fn test_init_s3() {
        let config = read_config("./tests/config.toml")
            .await
            .map_err(|e| {
                tracing::error!("read config failed: {}", e);
                e
            })
            .unwrap();
        let s3_op = init_s3(&config.s3).await.unwrap();

        let entries = s3_op.list("/backup/").await.unwrap();
        println!("{:#?}", entries);
    }
}
