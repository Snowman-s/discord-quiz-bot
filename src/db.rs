use sqlx::{FromRow, PgConnection, PgPool};

use crate::{mtg, ygo};

#[derive(sqlx::Type)]
#[sqlx(type_name = "quiz_type")]
pub enum QuizType {
    #[sqlx(rename = "ygo")]
    Ygo,
    #[sqlx(rename = "mtg")]
    Mtg,
}

#[derive(FromRow)]
pub struct Quiz {
    pub user_id: i64,
    pub quiz_type: QuizType,
}

pub async fn get_quiz_type(pool: &PgPool, user_id: &i64) -> Result<QuizType, sqlx::Error> {
    let data: Quiz = sqlx::query_as(r#"SELECT * FROM quiz WHERE user_id = $1"#)
        .bind(user_id)
        .fetch_one(pool)
        .await?;

    Ok(data.quiz_type)
}

pub async fn insert_quiz(
    tx: &mut PgConnection,
    user_id: &i64,
    quiz_type: &QuizType,
) -> Result<(), sqlx::Error> {
    delete_tx_quiz(tx, user_id).await?;

    sqlx::query(
        r#"
      INSERT INTO quiz (user_id, quiz_type)
      VALUES ($1, $2)
    "#,
    )
    .bind(&user_id)
    .bind(&quiz_type)
    .execute(&mut *tx)
    .await?;

    Ok(())
}

pub async fn delete_quiz(pool: &PgPool, user_id: &i64) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;

    delete_tx_quiz(&mut tx, user_id).await?;

    tx.commit().await?;

    Ok(())
}

pub(crate) async fn delete_tx_quiz(
    tx: &mut PgConnection,
    user_id: &i64,
) -> Result<(), sqlx::Error> {
    let willbe_deleted: Option<Quiz> = sqlx::query_as(r#"SELECT * FROM quiz WHERE user_id = $1"#)
        .bind(user_id)
        .fetch_optional(&mut *tx)
        .await?;

    match willbe_deleted {
        Some(quiz) => match quiz.quiz_type {
            QuizType::Ygo => ygo::db::delete_quiz(tx, user_id).await,
            QuizType::Mtg => mtg::db::delete_quiz(tx, user_id).await,
        }
        .unwrap(),
        None => {}
    }

    sqlx::query(r#"DELETE FROM quiz WHERE user_id = $1"#)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    Ok(())
}
