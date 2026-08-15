use std::collections::BTreeMap;

pub fn save_recipe(database: &mut BTreeMap<String, String>, name: &str, body: &str) {
    database.insert(name.to_owned(), body.to_owned());
}

pub fn read_recipe<'a>(database: &'a BTreeMap<String, String>, name: &str) -> Option<&'a str> {
    database.get(name).map(String::as_str)
}
