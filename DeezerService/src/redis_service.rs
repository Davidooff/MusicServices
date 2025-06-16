use redis::{AsyncCommands, RedisResult};
use regex::Regex;

// The struct is now public
pub struct RedisConnection<'a> {
    index_name: &'a str,
    connection: redis::aio::MultiplexedConnection,
    regex_id: &'a Regex,
}

impl<'a> RedisConnection<'a> {
    // The constructor is now public
    pub async fn try_new(
        endpoint: &str,
        index_name: &'a str,
        regex_id: &'a Regex,
    ) -> RedisResult<Self> {
        let client = redis::Client::open(endpoint)?;
        let connection = client.get_multiplexed_async_connection().await?;
        Ok(Self {
            connection,
            index_name,
            regex_id,
        })
    }

    /// Searches for an ID in Redis using FT.SEARCH based on a session ID, asynchronously.
    /// Returns Some(id) if found and matched by the regex, None if not found or no match.
    // The method is now public
    pub async fn get_id(&mut self, session_id: &str) -> Result<Option<String>, redis::RedisError> {
        // RediSearch queries should escape special characters.
        let query = format!("@session_id:{{{session_id}}}");

        // The result of FT.SEARCH is a vector where the first element is the count,
        // followed by key-value pairs of the documents.
        let result: Vec<String> = redis::cmd("FT.SEARCH")
            .arg(self.index_name)
            .arg(&query)
            .arg("NOCONTENT")
            .arg("LIMIT")
            .arg("0")
            .arg("1")
            .query_async(&mut self.connection)
            .await?;

        println!(
            "Redis FT.SEARCH result for session '{}': {:?}",
            session_id, result
        );

        // A successful search with NOCONTENT returns a count and the key.
        if result.len() == 2 {
            let key = &result[1];
            if let Some(caps) = self.regex_id.captures(key) {
                if let Some(regex_id) = caps.get(1) {
                    let id = regex_id.as_str();
                    return Ok(Some(id.to_owned()));
                }
            }
        }

        Ok(None)
    }
}
