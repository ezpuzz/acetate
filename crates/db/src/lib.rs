use sqlx::Connection;

pub fn get_connection(db_url: &str) {
    SqliteConnection::connect("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
