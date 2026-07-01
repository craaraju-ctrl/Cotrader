fn main() {
    let mut reg = rat_ecosystem::registry::ToolRegistry::new();
    rat_ecosystem::reasoning::register_all(&mut reg);
    rat_ecosystem::thinking::register_all(&mut reg);
    rat_ecosystem::indicators::register_all(&mut reg);
    rat_ecosystem::skills::register_all(&mut reg);
    rat_ecosystem::decision::register_all(&mut reg);
    rat_ecosystem::sentiment::register_all(&mut reg);
    rat_ecosystem::onchain::register_all(&mut reg);
    rat_ecosystem::news::register_all(&mut reg);
    rat_ecosystem::data_apis::register_all(&mut reg);
    rat_ecosystem::web_tools::register_all(&mut reg);
    rat_ecosystem::extra::register_all(&mut reg);
    println!("TOTAL TOOLS IN ECOSYSTEM: {}", reg.count());
    let mut counts: Vec<_> = reg.category_counts().into_iter().collect();
    counts.sort_by(|a, b| b.1.cmp(&a.1));
    for (cat, count) in &counts {
        println!("  {:?}: {}", cat, count);
    }
}
