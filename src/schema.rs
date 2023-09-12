// @generated automatically by Diesel CLI.

diesel::table! {
    messages (id) {
        id -> Int4,
        message -> Nullable<Text>,
        updated -> Nullable<Int4>,
    }
}
