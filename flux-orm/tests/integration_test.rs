// tests/integration_test.rs
// Comprehensive integration tests for FluxORM with PostgreSQL

use chrono::{DateTime, Utc};
use flux_orm::{Flux, Model, Query};
use flux_orm_derive::Model;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU32, Ordering};
use flux_orm::backend::errors::*;
use flux_orm::backend::common_models::*;
// ============================================================================
// TEST MODELS
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Model)]
#[flux(table = "groups", timestamps = true)]
pub struct Group {
    #[flux(primary_key)]
    pub id: Option<i64>,
    pub name: String,
    pub description: Option<String>,
    pub capacity: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Model)]
#[flux(table = "students", timestamps = true)]
pub struct Student {
    #[flux(primary_key)]
    pub id: Option<i64>,
    pub name: String,
    pub email: String,
    pub age: i32,
    pub group_id: Option<i64>,
    pub gpa: Option<f64>,
    pub enrolled: bool,
    pub metadata: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ============================================================================
// TEST SETUP AND TEARDOWN
// ============================================================================

// Global counter for unique test identifiers
static TEST_COUNTER: AtomicU32 = AtomicU32::new(0);

fn get_test_id() -> u32 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

async fn setup_database() -> Result<Flux> {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://fluxorm_test:test_pass@localhost:5433/flux_test".to_string());

    let db = Flux::postgres(&database_url)
        .await?
        .with_logging(true);

    // Полная очистка всех данных
    let _ = db.raw_execute("TRUNCATE TABLE students, groups RESTART IDENTITY CASCADE", &[]).await;

    // Убедимся, что таблицы существуют
    db.raw_execute(
        "CREATE TABLE IF NOT EXISTS groups (
            id SERIAL PRIMARY KEY,
            name VARCHAR(255) NOT NULL,
            description TEXT,
            capacity INTEGER NOT NULL DEFAULT 30,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )",
        &[],
    ).await?;

    db.raw_execute(
        "CREATE TABLE IF NOT EXISTS students (
            id SERIAL PRIMARY KEY,
            name VARCHAR(255) NOT NULL,
            email VARCHAR(255) NOT NULL UNIQUE,
            age INTEGER NOT NULL,
            group_id INTEGER REFERENCES groups(id) ON DELETE SET NULL,
            gpa DOUBLE PRECISION,
            enrolled BOOLEAN NOT NULL DEFAULT true,
            metadata JSONB,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )",
        &[],
    ).await?;

    // Создаём индексы только если их нет
    let _ = db.raw_execute(
        "CREATE INDEX IF NOT EXISTS idx_students_group_id ON students(group_id)",
        &[],
    ).await;

    let _ = db.raw_execute(
        "CREATE INDEX IF NOT EXISTS idx_students_email ON students(email)",
        &[],
    ).await;

    Ok(db)
}

async fn cleanup_database(db: &Flux) -> Result<()> {
    // Полная очистка всех данных после каждого теста
    db.raw_execute("TRUNCATE TABLE students, groups RESTART IDENTITY CASCADE", &[]).await?;
    Ok(())
}

// Хелпер для создания уникального email
fn unique_email(prefix: &str) -> String {
    format!("{}_{}_{}@example.com", prefix, get_test_id(), chrono::Utc::now().timestamp_millis())
}

// ============================================================================
// GROUP CRUD TESTS
// ============================================================================

#[tokio::test]
async fn test_group_create() -> Result<()> {
    let db = setup_database().await?;

    let group = Group {
        id: None,
        name: format!("Computer Science 101 {}", get_test_id()),
        description: Some("Introduction to Computer Science".to_string()),
        capacity: 30,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    let created_group = db.insert(group).await?;

    assert!(created_group.id.is_some());
    assert!(created_group.name.starts_with("Computer Science 101"));
    assert_eq!(created_group.capacity, 30);

    cleanup_database(&db).await?;
    Ok(())
}

#[tokio::test]
async fn test_group_read() -> Result<()> {
    let db = setup_database().await?;

    let group = Group {
        id: None,
        name: format!("Mathematics 201 {}", get_test_id()),
        description: Some("Advanced Mathematics".to_string()),
        capacity: 25,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    let created_group = db.insert(group).await?;
    let group_id = created_group.id.unwrap();

    let found_group = db.find::<Group>(group_id).await?;
    assert!(found_group.is_some());

    let found_group = found_group.unwrap();
    assert_eq!(found_group.id, Some(group_id));

    let group_or_fail = db.find_or_fail::<Group>(group_id).await?;
    assert!(group_or_fail.name.starts_with("Mathematics 201"));

    cleanup_database(&db).await?;
    Ok(())
}

#[tokio::test]
async fn test_group_update() -> Result<()> {
    let db = setup_database().await?;

    let mut group = Group {
        id: None,
        name: format!("Physics 301 {}", get_test_id()),
        description: Some("Classical Physics".to_string()),
        capacity: 20,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    let created_group = db.insert(group.clone()).await?;
    let group_id = created_group.id.unwrap();

    group.id = Some(group_id);
    group.name = format!("Physics 301 - Advanced {}", get_test_id());
    group.capacity = 25;

    db.update(group.clone()).await?;

    let updated_group = db.find_or_fail::<Group>(group_id).await?;
    assert!(updated_group.name.contains("Advanced"));
    assert_eq!(updated_group.capacity, 25);

    cleanup_database(&db).await?;
    Ok(())
}

#[tokio::test]
async fn test_group_delete() -> Result<()> {
    let db = setup_database().await?;

    let group = Group {
        id: None,
        name: format!("Chemistry 101 {}", get_test_id()),
        description: None,
        capacity: 30,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    let created_group = db.insert(group).await?;
    let group_id = created_group.id.unwrap();

    assert!(db.find::<Group>(group_id).await?.is_some());

    db.delete(created_group).await?;

    assert!(db.find::<Group>(group_id).await?.is_none());

    cleanup_database(&db).await?;
    Ok(())
}

#[tokio::test]
async fn test_group_query_all() -> Result<()> {
    let db = setup_database().await?;

    for i in 1..=5 {
        let group = Group {
            id: None,
            name: format!("Group {} {}", i, get_test_id()),
            description: Some(format!("Description {}", i)),
            capacity: 20 + i,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        db.insert(group).await?;
    }

    let all_groups = db.all::<Group>().await?;
    assert_eq!(all_groups.len(), 5);

    cleanup_database(&db).await?;
    Ok(())
}

// ============================================================================
// STUDENT CRUD TESTS
// ============================================================================

#[tokio::test]
async fn test_student_create() -> Result<()> {
    let db = setup_database().await?;

    let group = Group {
        id: None,
        name: format!("Test Group {}", get_test_id()),
        description: None,
        capacity: 30,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    let created_group = db.insert(group).await?;

    let student = Student {
        id: None,
        name: format!("Alice Johnson {}", get_test_id()),
        email: unique_email("alice"),
        age: 20,
        group_id: created_group.id,
        gpa: Some(3.8),
        enrolled: true,
        metadata: Some(serde_json::json!({"major": "Computer Science"})),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    let created_student = db.insert(student).await?;

    assert!(created_student.id.is_some());
    assert!(created_student.name.starts_with("Alice Johnson"));
    assert_eq!(created_student.age, 20);
    assert_eq!(created_student.gpa, Some(3.8));

    cleanup_database(&db).await?;
    Ok(())
}

#[tokio::test]
async fn test_student_read_with_relationships() -> Result<()> {
    let db = setup_database().await?;

    let group = Group {
        id: None,
        name: format!("CS Group {}", get_test_id()),
        description: None,
        capacity: 30,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    let created_group = db.insert(group).await?;
    let group_id = created_group.id.unwrap();

    let student = Student {
        id: None,
        name: format!("Bob Smith {}", get_test_id()),
        email: unique_email("bob"),
        age: 22,
        group_id: Some(group_id),
        gpa: Some(3.5),
        enrolled: true,
        metadata: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    let created_student = db.insert(student).await?;
    let student_id = created_student.id.unwrap();

    let found_student = db.find_or_fail::<Student>(student_id).await?;
    assert_eq!(found_student.group_id, Some(group_id));

    cleanup_database(&db).await?;
    Ok(())
}

#[tokio::test]
async fn test_student_update() -> Result<()> {
    let db = setup_database().await?;

    let mut student = Student {
        id: None,
        name: format!("Charlie Brown {}", get_test_id()),
        email: unique_email("charlie"),
        age: 19,
        group_id: None,
        gpa: Some(3.2),
        enrolled: true,
        metadata: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    let created_student = db.insert(student.clone()).await?;
    let student_id = created_student.id.unwrap();

    student.id = Some(student_id);
    student.age = 20;
    student.gpa = Some(3.5);
    student.enrolled = false;

    db.update(student.clone()).await?;

    let updated_student = db.find_or_fail::<Student>(student_id).await?;
    assert_eq!(updated_student.age, 20);
    assert_eq!(updated_student.gpa, Some(3.5));
    assert_eq!(updated_student.enrolled, false);

    cleanup_database(&db).await?;
    Ok(())
}

#[tokio::test]
async fn test_student_delete() -> Result<()> {
    let db = setup_database().await?;

    let student = Student {
        id: None,
        name: format!("Diana Prince {}", get_test_id()),
        email: unique_email("diana"),
        age: 21,
        group_id: None,
        gpa: Some(4.0),
        enrolled: true,
        metadata: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    let created_student = db.insert(student).await?;
    let student_id = created_student.id.unwrap();

    db.delete(created_student).await?;

    assert!(db.find::<Student>(student_id).await?.is_none());

    cleanup_database(&db).await?;
    Ok(())
}

// ============================================================================
// ADVANCED QUERY TESTS
// ============================================================================

#[tokio::test]
async fn test_query_where_conditions() -> Result<()> {
    let db = setup_database().await?;

    let test_id = get_test_id();
    let students_data = vec![
        ("Alice", 20, 3.8),
        ("Bob", 22, 3.5),
        ("Charlie", 19, 3.9),
        ("Diana", 21, 3.7),
        ("Eve", 23, 3.6),
    ];

    for (name, age, gpa) in students_data {
        let student = Student {
            id: None,
            name: format!("{} {}", name, test_id),
            email: unique_email(&format!("{}_{}", name.to_lowercase(), test_id)),
            age,
            group_id: None,
            gpa: Some(gpa),
            enrolled: true,
            metadata: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        db.insert(student).await?;
    }

    let students = db
        .query(Query::<Student>::new().where_eq("age", 20))
        .await?;
    assert_eq!(students.len(), 1);

    let students = db
        .query(Query::<Student>::new().where_gt("age", 21))
        .await?;
    assert_eq!(students.len(), 2);

    let students = db
        .query(Query::<Student>::new().where_lte("age", 20))
        .await?;
    assert_eq!(students.len(), 2);

    cleanup_database(&db).await?;
    Ok(())
}

#[tokio::test]
async fn test_query_order_by() -> Result<()> {
    let db = setup_database().await?;

    let test_id = get_test_id();
    for i in 1..=5 {
        let student = Student {
            id: None,
            name: format!("Student {} {}", i, test_id),
            email: unique_email(&format!("student{}_{}", i, test_id)),
            age: 18 + i,
            group_id: None,
            gpa: Some(3.0 + (i as f64) * 0.1),
            enrolled: true,
            metadata: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        db.insert(student).await?;
    }

    let students = db
        .query(Query::<Student>::new().order_by("age"))
        .await?;
    assert_eq!(students[0].age, 19);
    assert_eq!(students[4].age, 23);

    let students = db
        .query(Query::<Student>::new().order_by_desc("age"))
        .await?;
    assert_eq!(students[0].age, 23);
    assert_eq!(students[4].age, 19);

    cleanup_database(&db).await?;
    Ok(())
}

#[tokio::test]
async fn test_query_limit_offset() -> Result<()> {
    let db = setup_database().await?;

    let test_id = get_test_id();
    for i in 1..=10 {
        let student = Student {
            id: None,
            name: format!("Student {} {}", i, test_id),
            email: unique_email(&format!("student{}_{}", i, test_id)),
            age: 18 + i,
            group_id: None,
            gpa: Some(3.0),
            enrolled: true,
            metadata: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        db.insert(student).await?;
    }

    let students = db
        .query(Query::<Student>::new().limit(5))
        .await?;
    assert_eq!(students.len(), 5);

    let students = db
        .query(Query::<Student>::new().limit(3).offset(5))
        .await?;
    assert_eq!(students.len(), 3);

    cleanup_database(&db).await?;
    Ok(())
}

#[tokio::test]
async fn test_query_where_in() -> Result<()> {
    let db = setup_database().await?;

    let test_id = get_test_id();
    for age in vec![18, 20, 22, 24, 26] {
        let student = Student {
            id: None,
            name: format!("Student Age {} {}", age, test_id),
            email: unique_email(&format!("student_age_{}_{}", age, test_id)),
            age,
            group_id: None,
            gpa: Some(3.5),
            enrolled: true,
            metadata: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        db.insert(student).await?;
    }

    let students = db
        .query(Query::<Student>::new().where_in("age", vec![20, 24]))
        .await?;
    assert_eq!(students.len(), 2);

    cleanup_database(&db).await?;
    Ok(())
}

#[tokio::test]
async fn test_query_where_null() -> Result<()> {
    let db = setup_database().await?;

    let group = Group {
        id: None,
        name: format!("Elite Group {}", get_test_id()),
        description: Some("Top performers".to_string()),
        capacity: 20,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    let created_group = db.insert(group).await?;
    let group_id = created_group.id.unwrap();

    let test_id = get_test_id();
    for i in 1..=5 {
        let student = Student {
            id: None,
            name: format!("Student {} {}", i, test_id),
            email: unique_email(&format!("student{}_{}", i, test_id)),
            age: 20,
            group_id: if i % 2 == 0 { Some(group_id) } else { None },
            gpa: Some(3.5),
            enrolled: true,
            metadata: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        db.insert(student).await?;
    }

    let students = db
        .query(Query::<Student>::new().where_null("group_id"))
        .await?;
    assert_eq!(students.len(), 3);

    let students = db
        .query(Query::<Student>::new()
            .where_not_null("group_id")
            .where_field_eq(student_fields::GROUP_ID, group_id)
            .where_field_between(student_fields::AGE, 10, 30)
            .order_by_field(student_fields::NAME)
        )
        .await?;
    assert_eq!(students.len(), 2);

    cleanup_database(&db).await?;
    Ok(())
}

// ============================================================================
// BATCH AND TRANSACTION TESTS
// ============================================================================

#[tokio::test]
async fn test_batch_insert() -> Result<()> {
    let db = setup_database().await?;

    let test_id = get_test_id();
    let students: Vec<Student> = (1..=5)
        .map(|i| Student {
            id: None,
            name: format!("Batch Student {} {}", i, test_id),
            email: unique_email(&format!("batch{}_{}", i, test_id)),
            age: 18 + i,
            group_id: None,
            gpa: Some(3.0 + (i as f64) * 0.1),
            enrolled: true,
            metadata: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
        .collect();

    let inserted = db.batch_insert(students).await?;
    assert_eq!(inserted.len(), 5);

    for student in &inserted {
        assert!(student.id.is_some());
    }

    cleanup_database(&db).await?;
    Ok(())
}

#[tokio::test]
async fn test_transaction_commit() -> Result<()> {
    let db = setup_database().await?;

    let test_id = get_test_id();
    let result = db
        .transaction(|tx| {
            Box::pin(async move {
                let group_sql = "INSERT INTO groups (name, capacity, created_at, updated_at) VALUES ($1, $2, NOW(), NOW()) RETURNING id";
                let group_row = tx.fetch_one(group_sql, &[
                    Value::String(format!("Transaction Group {}", test_id)),
                    Value::I32(30),
                ]).await?;

                let group_id = group_row.get("id").and_then(|v| v.as_i64()).unwrap();

                let student_sql = "INSERT INTO students (name, email, age, group_id, enrolled, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, NOW(), NOW()) RETURNING id";
                let _student_row = tx.fetch_one(student_sql, &[
                    Value::String(format!("Transaction Student {}", test_id)),
                    Value::String(format!("transaction_{}@example.com", test_id)),
                    Value::I32(20),
                    Value::I64(group_id),
                    Value::Bool(true),
                ]).await?;

                Ok::<_, FluxError>(())
            })
        })
        .await;

    assert!(result.is_ok());

    let groups = db.all::<Group>().await?;
    assert_eq!(groups.len(), 1);

    let students = db.all::<Student>().await?;
    assert_eq!(students.len(), 1);

    cleanup_database(&db).await?;
    Ok(())
}

#[tokio::test]
async fn test_transaction_rollback() -> Result<()> {
    let db = setup_database().await?;

    let test_id = get_test_id();
    let result = db
        .transaction(|tx| {
            Box::pin(async move {
                let group_sql = "INSERT INTO groups (name, capacity, created_at, updated_at) VALUES ($1, $2, NOW(), NOW()) RETURNING id";
                let _group_row = tx.fetch_one(group_sql, &[
                    Value::String(format!("Rollback Group {}", test_id)),
                    Value::I32(30),
                ]).await?;

                Err::<(), _>(FluxError::QueryBuild("Intentional rollback".to_string()))
            })
        })
        .await;

    assert!(result.is_err());

    let groups = db.all::<Group>().await?;
    assert_eq!(groups.len(), 0);

    cleanup_database(&db).await?;
    Ok(())
}

// ============================================================================
// COUNT AND EXISTS TESTS
// ============================================================================

#[tokio::test]
async fn test_count() -> Result<()> {
    let db = setup_database().await?;

    let test_id = get_test_id();
    for i in 1..=7 {
        let student = Student {
            id: None,
            name: format!("Student {} {}", i, test_id),
            email: unique_email(&format!("student{}_{}", i, test_id)),
            age: 18 + i,
            group_id: None,
            gpa: Some(3.0),
            enrolled: i % 2 == 0,
            metadata: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        db.insert(student).await?;
    }

    let enrolled = db
        .count(Query::<Student>::new().where_eq("enrolled", true))
        .await?;
    assert_eq!(enrolled, 3);

    cleanup_database(&db).await?;
    Ok(())
}

#[tokio::test]
async fn test_exists() -> Result<()> {
    let db = setup_database().await?;

    let exists = db.exists(Query::<Student>::new()).await?;
    assert!(!exists);

    let student = Student {
        id: None,
        name: format!("Test Student {}", get_test_id()),
        email: unique_email("test"),
        age: 20,
        group_id: None,
        gpa: Some(3.5),
        enrolled: true,
        metadata: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    db.insert(student).await?;

    let exists = db.exists(Query::<Student>::new()).await?;
    assert!(exists);

    cleanup_database(&db).await?;
    Ok(())
}

// ============================================================================
// PAGINATION TEST
// ============================================================================

#[tokio::test]
async fn test_pagination() -> Result<()> {
    let db = setup_database().await?;

    let test_id = get_test_id();
    for i in 1..=25 {
        let student = Student {
            id: None,
            name: format!("Student {:02} {}", i, test_id),
            email: unique_email(&format!("student{:02}_{}", i, test_id)),
            age: 18 + (i % 5),
            group_id: None,
            gpa: Some(3.0 + (i as f64) * 0.01),
            enrolled: true,
            metadata: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        db.insert(student).await?;
    }

    let page1 = db
        .paginate(Query::<Student>::new().order_by("id"), 1, 10)
        .await?;
    assert_eq!(page1.page, 1);
    assert_eq!(page1.per_page, 10);
    assert_eq!(page1.total, 25);
    assert_eq!(page1.total_pages, 3);
    assert_eq!(page1.items.len(), 10);
    assert!(page1.has_next());
    assert!(!page1.has_prev());

    let page2 = db
        .paginate(Query::<Student>::new().order_by("id"), 2, 10)
        .await?;
    assert_eq!(page2.items.len(), 10);
    assert!(page2.has_next());
    assert!(page2.has_prev());

    let page3 = db
        .paginate(Query::<Student>::new().order_by("id"), 3, 10)
        .await?;
    assert_eq!(page3.items.len(), 5);
    assert!(!page3.has_next());
    assert!(page3.has_prev());

    cleanup_database(&db).await?;
    Ok(())
}

// ============================================================================
// UPSERT TEST
// ============================================================================

#[tokio::test]
async fn test_upsert() -> Result<()> {
    let db = setup_database().await?;

    let student = Student {
        id: None,
        name: format!("Upsert Student {}", get_test_id()),
        email: unique_email("upsert"),
        age: 20,
        group_id: None,
        gpa: Some(3.5),
        enrolled: true,
        metadata: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    let inserted = db.upsert(student).await?;
    assert!(inserted.id.is_some());
    let student_id = inserted.id.unwrap();

    let mut updated_student = inserted.clone();
    updated_student.age = 21;
    updated_student.gpa = Some(3.7);

    let upserted = db.upsert(updated_student).await?;
    assert_eq!(upserted.id, Some(student_id));

    let found = db.find_or_fail::<Student>(student_id).await?;
    assert_eq!(found.age, 21);
    assert_eq!(found.gpa, Some(3.7));

    cleanup_database(&db).await?;
    Ok(())
}

// ============================================================================
// JSON METADATA TEST
// ============================================================================

#[tokio::test]
async fn test_json_metadata() -> Result<()> {
    let db = setup_database().await?;

    let metadata = serde_json::json!({
        "major": "Computer Science",
        "minor": "Mathematics",
        "clubs": ["Chess Club", "Coding Club"],
        "honors": true
    });

    let student = Student {
        id: None,
        name: format!("JSON Student {}", get_test_id()),
        email: unique_email("json"),
        age: 21,
        group_id: None,
        gpa: Some(3.9),
        enrolled: true,
        metadata: Some(metadata.clone()),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    let created = db.insert(student).await?;
    let student_id = created.id.unwrap();

    let found = db.find_or_fail::<Student>(student_id).await?;
    assert!(found.metadata.is_some());

    let stored_metadata = found.metadata.unwrap();
    assert_eq!(stored_metadata["major"], "Computer Science");
    assert_eq!(stored_metadata["clubs"][0], "Chess Club");
    assert_eq!(stored_metadata["honors"], true);

    cleanup_database(&db).await?;
    Ok(())
}

// ============================================================================
// COMPLEX QUERY COMBINATION TEST
// ============================================================================

#[tokio::test]
async fn test_complex_query_combination() -> Result<()> {
    let db = setup_database().await?;

    let group = Group {
        id: None,
        name: format!("Elite Group {}", get_test_id()),
        description: Some("Top performers".to_string()),
        capacity: 20,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    let created_group = db.insert(group).await?;
    let group_id = created_group.id.unwrap();

    let test_id = get_test_id();
    let students_data = vec![
        ("Alice", 20, 3.9, Some(group_id), true),
        ("Bob", 22, 3.5, Some(group_id), true),
        ("Charlie", 19, 3.8, None, true),
        ("Diana", 21, 3.7, Some(group_id), false),
        ("Eve", 23, 3.6, None, true),
        ("Frank", 20, 3.95, Some(group_id), true),
        ("Grace", 22, 3.4, None, false),
    ];

    for (name, age, gpa, gid, enrolled) in students_data {
        let student = Student {
            id: None,
            name: format!("{} {}", name, test_id),
            email: unique_email(&format!("{}_{}", name.to_lowercase(), test_id)),
            age,
            group_id: gid,
            gpa: Some(gpa),
            enrolled,
            metadata: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        db.insert(student).await?;
    }

    let query = Query::<Student>::new()
        .where_eq("enrolled", true)
        .where_not_null("group_id")
        .where_gt("gpa", 3.7)
        .order_by_desc("gpa")
        .limit(10);

    let results = db.query(query).await?;

    assert_eq!(results.len(), 2);
    assert!(results[0].gpa.unwrap() > 3.7);
    assert!(results[1].gpa.unwrap() > 3.7);

    cleanup_database(&db).await?;
    Ok(())
}

// ============================================================================
// RAW QUERY TESTS
// ============================================================================

#[tokio::test]
async fn test_raw_query() -> Result<()> {
    let db = setup_database().await?;

    let test_id = get_test_id();
    for i in 1..=5 {
        let student = Student {
            id: None,
            name: format!("Student {} {}", i, test_id),
            email: unique_email(&format!("student{}_{}", i, test_id)),
            age: 18 + i,
            group_id: None,
            gpa: Some(3.0 + (i as f64) * 0.1),
            enrolled: true,
            metadata: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        db.insert(student).await?;
    }

    let sql = "SELECT * FROM students WHERE age >= $1 ORDER BY age DESC";
    let students: Vec<Student> = db
        .raw_query(sql, &[Value::I32(20)])
        .await?;

    assert_eq!(students.len(), 4);
    assert_eq!(students[0].age, 23);

    cleanup_database(&db).await?;
    Ok(())
}

#[tokio::test]
async fn test_raw_execute() -> Result<()> {
    let db = setup_database().await?;

    let student = Student {
        id: None,
        name: format!("Raw Test {}", get_test_id()),
        email: unique_email("raw"),
        age: 20,
        group_id: None,
        gpa: Some(3.5),
        enrolled: true,
        metadata: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    let created = db.insert(student).await?;
    let student_id = created.id.unwrap();

    let sql = "UPDATE students SET age = $1, gpa = $2 WHERE id = $3";
    let affected = db
        .raw_execute(
            sql,
            &[
                Value::I32(25),
                Value::F64(3.9),
                Value::I64(student_id),
            ],
        )
        .await?;

    assert_eq!(affected, 1);

    let updated = db.find_or_fail::<Student>(student_id).await?;
    assert_eq!(updated.age, 25);
    assert_eq!(updated.gpa, Some(3.9));

    cleanup_database(&db).await?;
    Ok(())
}

// ============================================================================
// FIRST METHOD TEST
// ============================================================================

#[tokio::test]
async fn test_first() -> Result<()> {
    let db = setup_database().await?;

    let first = db.first(Query::<Student>::new()).await?;
    assert!(first.is_none());

    let test_id = get_test_id();
    for i in 1..=5 {
        let student = Student {
            id: None,
            name: format!("Student {} {}", i, test_id),
            email: unique_email(&format!("student{}_{}", i, test_id)),
            age: 18 + i,
            group_id: None,
            gpa: Some(3.0),
            enrolled: true,
            metadata: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        db.insert(student).await?;
    }

    let first = db.first(Query::<Student>::new().order_by("age")).await?;
    assert!(first.is_some());
    assert_eq!(first.unwrap().age, 19);

    let first = db
        .first(Query::<Student>::new().where_gt("age", 21).order_by("age"))
        .await?;
    assert!(first.is_some());
    assert_eq!(first.unwrap().age, 22);

    cleanup_database(&db).await?;
    Ok(())
}

// ============================================================================
// EDGE CASES AND ERROR HANDLING
// ============================================================================

#[tokio::test]
async fn test_not_found_error() -> Result<()> {
    let db = setup_database().await?;

    let result = db.find::<Student>(99999).await?;
    assert!(result.is_none());

    let result = db.find_or_fail::<Student>(99999).await;
    assert!(result.is_err());

    match result {
        Err(FluxError::NotFound) => {
            // Expected error
        }
        _ => panic!("Expected NotFound error"),
    }

    cleanup_database(&db).await?;
    Ok(())
}

#[tokio::test]
async fn test_unique_constraint_violation() -> Result<()> {
    let db = setup_database().await?;

    let email = unique_email("unique");

    let student1 = Student {
        id: None,
        name: format!("John Doe {}", get_test_id()),
        email: email.clone(),
        age: 20,
        group_id: None,
        gpa: Some(3.5),
        enrolled: true,
        metadata: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    db.insert(student1).await?;

    let student2 = Student {
        id: None,
        name: format!("Jane Doe {}", get_test_id()),
        email: email.clone(),
        age: 21,
        group_id: None,
        gpa: Some(3.7),
        enrolled: true,
        metadata: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    let result = db.insert(student2).await;
    assert!(result.is_err());

    cleanup_database(&db).await?;
    Ok(())
}

#[tokio::test]
async fn test_update_without_id() -> Result<()> {
    let db = setup_database().await?;

    let student = Student {
        id: None,
        name: format!("Test {}", get_test_id()),
        email: unique_email("test"),
        age: 20,
        group_id: None,
        gpa: Some(3.5),
        enrolled: true,
        metadata: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    let result = db.update(student).await;
    assert!(result.is_err());

    match result {
        Err(FluxError::NoId) => {
            // Expected error
        }
        _ => panic!("Expected NoId error"),
    }

    cleanup_database(&db).await?;
    Ok(())
}

#[tokio::test]
async fn test_empty_batch_insert() -> Result<()> {
    let db = setup_database().await?;

    let empty_vec: Vec<Student> = vec![];
    let result = db.batch_insert(empty_vec).await?;

    assert_eq!(result.len(), 0);

    cleanup_database(&db).await?;
    Ok(())
}

// ============================================================================
// RELATIONSHIP SIMULATION TESTS
// ============================================================================

#[tokio::test]
async fn test_find_students_by_group() -> Result<()> {
    let db = setup_database().await?;

    let test_id = get_test_id();

    let group1 = Group {
        id: None,
        name: format!("Group A {}", test_id),
        description: None,
        capacity: 30,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    let group2 = Group {
        id: None,
        name: format!("Group B {}", test_id),
        description: None,
        capacity: 30,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    let g1 = db.insert(group1).await?;
    let g2 = db.insert(group2).await?;

    for i in 1..=3 {
        let student = Student {
            id: None,
            name: format!("Student Group A {} {}", i, test_id),
            email: unique_email(&format!("group_a_{}_{}", i, test_id)),
            age: 20,
            group_id: g1.id,
            gpa: Some(3.5),
            enrolled: true,
            metadata: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        db.insert(student).await?;
    }

    for i in 1..=2 {
        let student = Student {
            id: None,
            name: format!("Student Group B {} {}", i, test_id),
            email: unique_email(&format!("group_b_{}_{}", i, test_id)),
            age: 20,
            group_id: g2.id,
            gpa: Some(3.5),
            enrolled: true,
            metadata: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        db.insert(student).await?;
    }

    let group_a_students = db
        .query(Query::<Student>::new().where_eq("group_id", g1.id.unwrap()))
        .await?;
    assert_eq!(group_a_students.len(), 3);

    let group_b_students = db
        .query(Query::<Student>::new().where_eq("group_id", g2.id.unwrap()))
        .await?;
    assert_eq!(group_b_students.len(), 2);

    cleanup_database(&db).await?;
    Ok(())
}

#[tokio::test]
async fn test_cascade_delete_simulation() -> Result<()> {
    let db = setup_database().await?;

    let group = Group {
        id: None,
        name: format!("Delete Test Group {}", get_test_id()),
        description: None,
        capacity: 30,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    let created_group = db.insert(group).await?;
    let group_id = created_group.id.unwrap();

    let test_id = get_test_id();
    for i in 1..=3 {
        let student = Student {
            id: None,
            name: format!("Cascade Student {} {}", i, test_id),
            email: unique_email(&format!("cascade{}_{}", i, test_id)),
            age: 20,
            group_id: Some(group_id),
            gpa: Some(3.5),
            enrolled: true,
            metadata: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        db.insert(student).await?;
    }

    let students_before = db
        .query(Query::<Student>::new().where_eq("group_id", group_id))
        .await?;
    assert_eq!(students_before.len(), 3);

    db.delete(created_group).await?;

    let students_after = db
        .query(Query::<Student>::new().where_null("group_id"))
        .await?;
    assert!(students_after.len() >= 3);

    cleanup_database(&db).await?;
    Ok(())
}

// ============================================================================
// PERFORMANCE AND STRESS TESTS
// ============================================================================

#[tokio::test]
async fn test_large_batch_insert() -> Result<()> {
    let db = setup_database().await?;

    let test_id = get_test_id();
    let students: Vec<Student> = (1..=100)
        .map(|i| Student {
            id: None,
            name: format!("Batch Student {:03} {}", i, test_id),
            email: unique_email(&format!("batch{:03}_{}", i, test_id)),
            age: 18 + (i % 10),
            group_id: None,
            gpa: Some(3.0 + ((i % 100) as f64) * 0.01),
            enrolled: i % 2 == 0,
            metadata: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
        .collect();

    let inserted = db.batch_insert(students).await?;
    assert_eq!(inserted.len(), 100);

    let count = db.count(Query::<Student>::new()).await?;
    assert_eq!(count, 100);

    cleanup_database(&db).await?;
    Ok(())
}

#[tokio::test]
async fn test_multiple_queries_performance() -> Result<()> {
    let db = setup_database().await?;

    let test_id = get_test_id();
    for i in 1..=50 {
        let student = Student {
            id: None,
            name: format!("Perf Student {} {}", i, test_id),
            email: unique_email(&format!("perf{}_{}", i, test_id)),
            age: 18 + (i % 10),
            group_id: None,
            gpa: Some(3.0 + (i as f64) * 0.01),
            enrolled: true,
            metadata: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        db.insert(student).await?;
    }

    let _q1 = db.query(Query::<Student>::new().where_gt("age", 20)).await?;
    let _q2 = db.query(Query::<Student>::new().where_lte("age", 22)).await?;
    let _q3 = db.query(Query::<Student>::new().order_by("gpa").limit(10)).await?;
    let _q4 = db.count(Query::<Student>::new()).await?;
    let _q5 = db.exists(Query::<Student>::new().where_eq("age", 25)).await?;

    cleanup_database(&db).await?;
    Ok(())
}

// ============================================================================
// PING AND CONNECTION TESTS
// ============================================================================

#[tokio::test]
async fn test_database_ping() -> Result<()> {
    let db = setup_database().await?;

    let result = db.ping().await;
    assert!(result.is_ok());

    cleanup_database(&db).await?;
    Ok(())
}