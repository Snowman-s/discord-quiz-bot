use sqlx::{FromRow, PgPool};
use std::fmt::Write;

#[derive(FromRow)]
struct Quiz {
    pub user_id: i32,
    pub konami_id: i32,
    pub card_name: String,
    pub card_text: String,
}

pub(crate) async fn new_quiz(
    pool: &PgPool,
    user_id: &i64,
    konami_id: &i64,
    card_name: &String,
    card_text: &String,
) -> Result<String, sqlx::Error> {
    let mut tx = pool.begin().await?;

    sqlx::query(r#"DELETE FROM ygo_quiz WHERE user_id = $1"#)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query(
        r#"
      INSERT INTO ygo_quiz (user_id, konami_id, card_name, card_text)
      VALUES ($1, $2, $3, $4)
    "#,
    )
    .bind(&user_id)
    .bind(&konami_id)
    .bind(&card_name)
    .bind(&card_text)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(format!(
        "Start quiz about `{}` for `{}`",
        card_name, user_id
    ))
}
