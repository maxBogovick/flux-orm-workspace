#[cfg(test)]
mod tests {
    use chrono::{DateTime, TimeZone, Utc};
    use flux_orm::backend::common_models::Value;
    use flux_orm::backend::errors::Result;
    use uuid::{Uuid, uuid};

    // Constants for consistent testing data
    const VALID_UUID_STR: &str = "550e8400-e29b-41d4-a716-446655440000";
    const VALID_DATE_STR: &str = "2023-01-01T00:00:00Z";

    // ============================================================================
    // 1. CONSTRUCTION TESTS (FROM TRAIT)
    // ============================================================================
    mod construction {
        use serde_json::json;
        use super::*;

        #[test]
        fn should_create_from_primitives() {
            assert_eq!(Value::from(true), Value::Bool(true));
            assert_eq!(Value::from(42_i32), Value::I32(42));
            assert_eq!(Value::from(100_i64), Value::I64(100));
            assert_eq!(Value::from(1.5_f64), Value::F64(1.5));
        }

        #[test]
        fn should_create_from_string_slice_and_owned() {
            let expected = Value::String("hello".to_string());
            assert_eq!(Value::from("hello"), expected);
            assert_eq!(Value::from("hello".to_string()), expected);
        }

        #[test]
        fn should_create_from_complex_types() {
            // UUID
            let uuid_val = uuid!("550e8400-e29b-41d4-a716-446655440000");
            assert_eq!(Value::from(uuid_val), Value::Uuid(uuid_val));

            // DateTime
            let dt = Utc.with_ymd_and_hms(2023, 1, 1, 0, 0, 0).unwrap();
            assert_eq!(Value::from(dt), Value::DateTime(dt));

            // JSON
            let json_val = json!({"key": "value"});
            assert_eq!(Value::from(json_val.clone()), Value::Json(json_val));
        }

        #[test]
        fn should_handle_option_types() {
            let some_val: Option<i32> = Some(42);
            let none_val: Option<i32> = None;

            assert_eq!(Value::from(some_val), Value::I32(42));
            assert_eq!(Value::from(none_val), Value::Null);
        }
    }

    // ============================================================================
    // 2. COERCION TESTS (TRY_FROM TRAIT)
    // ============================================================================
    mod coercion {
        use super::*;

        #[test]
        fn should_convert_value_to_primitives() {
            let val_bool = Value::Bool(true);
            let val_i32 = Value::I32(42);
            let val_f64 = Value::F64(10.5);

            assert_eq!(bool::try_from(val_bool).unwrap(), true);
            assert_eq!(i32::try_from(val_i32).unwrap(), 42);
            assert_eq!(f64::try_from(val_f64).unwrap(), 10.5);
        }

        #[test]
        fn should_coerce_numeric_types() {
            // i64 -> i32
            let big_int = Value::I64(42);
            assert_eq!(i32::try_from(big_int).unwrap(), 42);

            // f64 -> f32
            let big_float = Value::F64(42.5);
            assert_eq!(f32::try_from(big_float).unwrap(), 42.5);
        }

        #[test]
        fn should_parse_strings_into_types() {
            // UUID parsing
            let uuid_val = Value::String(VALID_UUID_STR.to_string());
            let parsed_uuid: Uuid = uuid_val.try_into().unwrap();
            assert_eq!(parsed_uuid, Uuid::parse_str(VALID_UUID_STR).unwrap());

            // DateTime parsing
            let date_val = Value::String(VALID_DATE_STR.to_string());
            let parsed_date: DateTime<Utc> = date_val.try_into().unwrap();
            assert_eq!(parsed_date, Utc.with_ymd_and_hms(2023, 1, 1, 0, 0, 0).unwrap());

            // JSON parsing
            let json_str = r#"{"foo": "bar"}"#;
            let json_val = Value::String(json_str.to_string());
            let parsed_json: serde_json::Value = json_val.try_into().unwrap();
            assert_eq!(parsed_json["foo"], "bar");
        }

        #[test]
        fn should_fail_on_invalid_type_conversions() {
            let val = Value::String("not a boolean".to_string());
            let res: Result<bool> = val.try_into();
            assert!(res.is_err(), "Should fail converting arbitrary string to bool");
        }

        #[test]
        fn should_fail_on_malformed_strings() {
            let val = Value::String("not-a-uuid".to_string());
            let res: Result<Uuid> = val.try_into();
            assert!(res.is_err(), "Should fail parsing invalid UUID string");
        }
    }

    // ============================================================================
    // 3. ACCESSOR TESTS (GETTERS)
    // ============================================================================
    mod accessors {
        use super::*;

        #[test]
        fn should_check_null_status() {
            assert!(Value::Null.is_null());
            assert!(!Value::I32(0).is_null());
        }

        #[test]
        fn should_get_simple_types() {
            let i_val = Value::I32(100);
            assert_eq!(i_val.as_i32(), Some(100));
            // Cross-type access
            assert_eq!(i_val.as_i64(), Some(100));
        }

        #[test]
        fn should_get_cloned_types() {
            let s_val = Value::String("test".to_string());
            assert_eq!(s_val.as_string(), Some("test".to_string()));
        }

        #[test]
        fn should_get_references() {
            let s_val = Value::String("test".to_string());
            let b_val = Value::Bytes(vec![1, 2, 3]);

            assert_eq!(s_val.as_str(), Some("test"));
            assert_eq!(b_val.as_bytes(), Some(&[1, 2, 3][..]));
        }

        #[test]
        fn should_coerce_numerics_safely() {
            // I32 fits in I64
            assert_eq!(Value::I32(10).as_i64(), Some(10));

            // F64 fits in F32 (precision loss check omitted for simplicity, just type check)
            assert_eq!(Value::F64(10.0).as_f32(), Some(10.0));
        }

        #[test]
        fn should_coerce_boolean_logic() {
            // Integer to Bool logic: 0 is false, non-zero is true
            assert_eq!(Value::I32(1).as_bool(), Some(true));
            assert_eq!(Value::I32(0).as_bool(), Some(false));
            assert_eq!(Value::I64(100).as_bool(), Some(true));
        }
    }

    // ============================================================================
    // 4. EDGE CASES & BOUNDARIES
    // ============================================================================
    mod edge_cases {
        use super::*;

        #[test]
        fn accessor_should_return_none_on_overflow() {
            // i16::MAX is 32767.
            // If we have an i32 larger than that, as_i16() should return None.
            let big_int = Value::I32(50_000);

            assert_eq!(
                big_int.as_i16(),
                None,
                "Should return None because 50,000 does not fit in i16"
            );
        }

        #[test]
        fn accessor_should_handle_negative_boundaries() {
            let negative = Value::I32(-1);
            assert_eq!(negative.as_i16(), Some(-1));

            let too_small = Value::I32(-50_000);
            assert_eq!(too_small.as_i16(), None);
        }

        #[test]
        fn numeric_try_from_truncation_behavior() {
            // NOTE: The current macro implementation uses `as` casting for TryFrom.
            // This implies implicit truncation (standard Rust behavior for `as`).
            // While `as_i16()` performs a check, `i16::try_from()` might strict cast depending on impl.
            // Based on your macro `Ok(v as $rust_type)`, this will wrap/truncate.

            let val = Value::I32(50_000); // Binary: ...1100001101010000
            let res = i16::try_from(val);

            // 50000 as i16 is -15536 due to overflow wrapping in Rust's `as`
            assert!(res.is_ok());
            assert_ne!(res.unwrap() as i32, 50_000);
        }
    }
}