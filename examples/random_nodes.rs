use clap::Parser;
use fake::faker::name::en::Name; // Standard English names
use fake::Fake;
use ruzu::Database;
use std::time::Instant;

/// A tool to batch insert fake people into the Ruzu database
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// How many fake people to generate?
    #[arg(short, long, default_value_t = 100)]
    count: usize,
}

fn main() {
    let args = Args::parse();

    println!("Initializing database...");
    let mut db = Database::new();

    // Create the schema
    let _ =
        db.execute("CREATE NODE TABLE Person(id INT64, name STRING, age INT64, PRIMARY KEY(id))");

    println!("Generating {} insert queries...", args.count);

    // Optimization: Pre-allocate memory so the Vector doesn't have to resize constantly
    let mut queries = Vec::with_capacity(args.count);

    for i in 0..args.count {
        let fake_name: String = Name().fake();
        let fake_age: i64 = (18..90).fake();

        let create_query = format!(
            "CREATE (:Person {{id: {}, name: '{}', age: {}}})",
            i,
            fake_name.replace('\'', ""),
            fake_age
        );

        // Push string to the list instead of executing it
        queries.push(create_query);
    }

    // --- PHASE 2: INSERTION & TIMING ---
    println!("Starting database insertion...");

    // 1. Start the clock
    let start = Instant::now();

    // 2. Run the loop (No generation logic here, just I/O)
    for query in queries {
        db.execute(&query).unwrap();
    }

    // 3. Stop the clock
    let duration = start.elapsed();

    println!("Done!");
    println!("Time taken for insert: {duration:?}");

    // Calculate simple operations per second (optional)
    // Avoid division by zero if it was too fast
    if duration.as_secs_f64() > 0.0 {
        #[allow(clippy::cast_precision_loss)]
        let speed = args.count as f64 / duration.as_secs_f64();
        println!("Speed: {speed:.2} inserts/sec");
    }

    // Verify a few records
    println!("Verifying first 5 records...");
    let result = db.execute("MATCH (p:Person) RETURN p.name, p.age").unwrap();

    // This safely takes the first 5, or fewer if the Vec is smaller
    for row in result.rows.iter().take(5) {
        let name = row.get("p.name").unwrap();
        println!(" - {name:?}");
    }
}
