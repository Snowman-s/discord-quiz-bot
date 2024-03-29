use sqlx::{FromRow, PgConnection, PgPool};

use crate::db::insert_quiz;

#[derive(FromRow)]
pub struct Quiz {
    pub user_id: i64,
    pub konami_id: i64,
    pub card_name: String,
    pub card_name_ruby: String,
    pub card_text: String,
}

pub(crate) async fn new_quiz(
    pool: &PgPool,
    user_id: &i64,
    konami_id: &i64,
    card_name: &String,
    card_name_ruby: &String,
    card_text: &String,
) -> Result<String, sqlx::Error> {
    let mut tx = pool.begin().await?;

    crate::db::delete_tx_quiz(&mut tx, user_id).await?;

    sqlx::query(
        r#"
      INSERT INTO ygo_quiz (user_id, konami_id, card_name, card_name_ruby, card_text)
      VALUES ($1, $2, $3, $4, $5)
    "#,
    )
    .bind(&user_id)
    .bind(&konami_id)
    .bind(&card_name)
    .bind(&card_name_ruby)
    .bind(&card_text)
    .execute(&mut *tx)
    .await?;

    insert_quiz(&mut *tx, user_id, &crate::db::QuizType::Ygo).await?;

    tx.commit().await?;

    Ok(format!(
        "Start quiz about `{}` for `{}`",
        card_name, user_id
    ))
}

pub(crate) async fn get_quiz(pool: &PgPool, user_id: &i64) -> Result<Quiz, sqlx::Error> {
    let data: Quiz = sqlx::query_as(r#"SELECT * FROM ygo_quiz WHERE user_id = $1"#)
        .bind(user_id)
        .fetch_one(pool)
        .await?;

    Ok(data)
}

pub(crate) async fn delete_quiz(pool: &mut PgConnection, user_id: &i64) -> Result<(), sqlx::Error> {
    sqlx::query(r#"DELETE FROM ygo_quiz WHERE user_id = $1"#)
        .bind(user_id)
        .execute(pool)
        .await?;

    Ok(())
}
