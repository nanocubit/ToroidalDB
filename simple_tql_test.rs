// Simple test to verify TQL module structure
mod src {
    pub mod tql {
        pub mod ast;
        pub mod parser;
        pub mod executor;
        
        // Include the actual modules
        include!("src/tql/ast.rs");
        include!("src/tql/parser.rs");
        // include!("src/tql/executor.rs");  // This might be too large
    }
}

fn main() {
    println!("TQL modules compiled successfully");
    
    // Test basic parsing functionality
    if let Ok((remaining, parsed)) = src::tql::parser::parse_query("MATCH (n) RETURN n") {
        println!("Parsed query successfully, {} remaining", remaining.len());
    } else {
        println!("Basic parsing failed");
    }
}