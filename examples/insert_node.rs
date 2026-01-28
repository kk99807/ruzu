use clap::Parser;
use ruzu::Database;

/// A simple tool to insert people into the Ruzu database
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// The name of the person to insert
    #[arg(short, long)]
    name: String,

    /// The age of the person
    #[arg(short, long)]
    age: i64,
}

fn main() {
    // 1. This one line parses the CLI, checks types, and handles errors
    let args = Args::parse();

    println!("Initializing database...");
    let mut db = Database::new();

    db.execute("CREATE NODE TABLE Person(name STRING, age INT64, PRIMARY KEY(name))")
        .unwrap();

    // 2. We use data from 'args' directly.
    // args.name is guaranteed to be a String, args.age is guaranteed to be an i64.
    let create_query = format!(
        "CREATE (:Person {{name: '{}', age: {}}})",
        args.name, args.age
    );

    db.execute(&create_query).unwrap();

    println!("Successfully inserted: {} (Age: {})", args.name, args.age);

    // Verify
    let result = db.execute("MATCH (p:Person) RETURN p.name, p.age").unwrap();

    for row in &result.rows {
        // Note: Depending on ruzu's implementation, you might need to handle Types here
        let name = row.get("p.name").unwrap();
        let age = row.get("p.age").unwrap();
        println!("Found in DB: {name:?}: {age:?}");
    }
}
