// Copyright 2023, Takahiro Yoshizawa
// Use of this source code is permitted under the license.
// The license can be viewed in the LICENSE file located in the project's top directory.

// Author: Takahiro Yoshizawa
// Description: A Rust program to process contact information using Google People API
// and export it to the AddressBook of Alpine Email Program.

// Import necessary crates and modules
use yup_oauth2::{InstalledFlowAuthenticator, InstalledFlowReturnMethod, read_application_secret, authenticator::Authenticator}; // Modules for OAuth2 authentication
use hyper::client::{Client, HttpConnector}; // For HTTP client operations
use hyper_rustls::HttpsConnector; // For HTTPS support
use std::str::FromStr; // For converting strings to types
use google_people1::{PeopleService, FieldMask}; // For using Google People API
use std::collections::HashSet; // For set operations on data
use csv::WriterBuilder; // For writing CSV files
use std::io;       // Import the 'io' module for input/output functionality
use std::path::Path;   // Import the 'Path' module to work with file paths
use std::env;      // Import the 'env' module to work with environment variables

// Function to print the help message for the application
fn print_help() {
    // Print the overall description of the application
    println!("Application Description:");
    // Explain that the application retrieves contact information using the Google People API
    println!("\tThis application retrieves contact information using Google People API,");
    // Explain that the retrieved information is exported in a specific format to a file in the user's home directory
    println!("\tand exports it in the format of Alpine Email Program's AddressBook to the user's home directory.");
    // Placeholder for additional detailed descriptions and usage instructions
    // Write other detailed descriptions and usage instructions here
}

// Asynchronous function to authenticate with Google API and return Authenticator
async fn get_auth() -> Result<Authenticator<HttpsConnector<HttpConnector>>, Box<dyn std::error::Error>> {
    // Retrieve the user's home directory
    let home_dir = dirs::home_dir().expect("Home directory not found");
    // Set the file paths for authentication information and token cache
    let secret_file = home_dir.join(".client_secret.json");
    let token_cache_file = home_dir.join(".token_cache.json");

    // Read Google API authentication information from file
    let secret = read_application_secret(secret_file).await?;

    // Build an HTTP client compatible with HTTPS
    let client = Client::builder().build(HttpsConnector::with_native_roots());

    // Construct and return OAuth2 authentication flow
    let auth = InstalledFlowAuthenticator::builder(secret,  InstalledFlowReturnMethod::HTTPRedirect)
        .persist_tokens_to_disk(token_cache_file)
        .hyper_client(client)
        .build()
        .await?;

    Ok(auth)
}

// Function to generate a nickname based on the given name and number of email addresses
fn generate_nickname(name: &str, email_count: usize, existing_nicknames: &mut HashSet<String>) -> String {
    // Obtain the last part of the name (assumed to be the surname) and create a base nickname
    let last_name_part = name.split_whitespace().last().unwrap_or("Unknown").to_string();
    let base_nickname = last_name_part;

    // Generate a unique nickname if there are multiple email addresses
    if email_count > 1 {
        let mut counter = 1;
        let mut nickname = format!("{}{:02}", base_nickname, counter);
        while existing_nicknames.contains(&nickname) {
            counter += 1;
            nickname = format!("{}{:02}", base_nickname, counter);
        }
        existing_nicknames.insert(nickname.clone());
        nickname
    } else {
        // Use the base nickname if there is only one email address
        existing_nicknames.insert(base_nickname.clone());
        base_nickname
    }
}

// Main function (asynchronous)
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Retrieve command-line arguments
    let args: Vec<String> = env::args().collect();

    // Check for --help option
    if args.contains(&"--help".to_string()) {
        print_help();
        return Ok(());
    }

    // Continue processing if authentication succeeds
    let auth = match get_auth().await{
        Ok(a) => a,
        Err(e) => {
            eprintln!("Authentication failed: {}", e);
            Err(e)
        }?
    };

    // Initialize PeopleService (Google People API client)
    let service = PeopleService::new(Client::builder().build(HttpsConnector::with_native_roots()), auth);

    // Set fields to be retrieved from Google People API
    let field_mask = match FieldMask::from_str("names,emailAddresses") {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Failed to retrieve fields: {}", e);
            Err(Box::new(e) as Box<dyn std::error::Error>)
        }?
    };

    // Retrieve contact information using Google People API
    // results is a tuple (Response<Body>, ListConnectionsResponse)
    // The return value of doit() is Result<(Response<Body>, ListConnectionsResponse)>
    let results = match service.people().connections_list("people/me")
       .page_size(1000)
       .person_fields(field_mask)
       .doit().await {
           Ok(r) => r,
           Err(e) => {
                eprintln!("Failed to retrieve contact information: {}", e);
                Err(Box::new(e) as Box<dyn std::error::Error>)
           }?
        };

    // HashSet to store generated nicknames
    let mut existing_nicknames = HashSet::new();
    // Specify the location for saving the CSV file
    let home_dir = dirs::home_dir().ok_or_else(|| {
        // This block is a closure. An error is being generated here.
        eprintln!("Home directory not found");
        Box::new(std::io::Error::new(std::io::ErrorKind::NotFound, "Home directory not found")) as Box<dyn std::error::Error>
    })?;

    let addressbook_path = home_dir.join(".addressbook");

    // Check if the file exists
    if Path::new(&addressbook_path).exists() {
        println!("The file exists. Overwrite? [y/N]");
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        if input.trim().to_lowercase() != "y" {
            println!("Operation cancelled.");
            return Ok(());
        }
    }

    // Initialize the CSV file writer (tab-separated)
    let mut writer = match WriterBuilder::new()
        .delimiter(b'\t')
        .from_path(addressbook_path) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("Failed to initialize address book: {}", e);
                // Here the process ends, or the error is propagated upwards
                Err(Box::new(e) as Box<dyn std::error::Error>)
            }?
    };

    // Process based on retrieved contact information
    // results.1 is of type ListConnectionsResponse
    if let Some(connections) = results.1.connections {
        for person in connections {
            // Retrieve each person's name and email addresses
            let names = person.names.unwrap_or_else(Vec::new);
            let emails = person.email_addresses.unwrap_or_else(Vec::new);

            // Process only if there are names
            if !names.is_empty() {
                // name is &str
                // display_name is Option<String>
                let name = names[0].display_name.as_ref().map(|s| s.as_str()).unwrap_or("default");
                let email_count = emails.len();

                // Assign nicknames to each email address and write to CSV
                for email in emails {
                    let email_address = email.value.unwrap_or_default();
                    let nickname = generate_nickname(&name, email_count, &mut existing_nicknames);
                      writer.write_record(&[&nickname, name, &email_address])
                          .map_err(|e| {
                              eprintln!("Failed to write to the address book: {}", e);
                              e
                          })?;
                }
            }
        }
    };

    // Complete writing to the CSV file
    writer.flush().map_err(|e| {
        eprintln!("Failed to complete writing to the address book: {}", e);
        e
    })?;

    println!("The address book has been exported to the home directory.");
    Ok(())
}
