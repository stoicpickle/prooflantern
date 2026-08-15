pub fn add_recipe(name: &str, recipes: &mut Vec<String>) {
    if !name.trim().is_empty() {
        recipes.push(name.trim().to_owned());
    }
}
