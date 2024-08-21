use actix_files::Files;
use actix_web::{get, web, App, Error, HttpRequest, HttpResponse, HttpServer, Responder};
use chrono::{Datelike, Utc};
use dotenv::dotenv;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::boxed::Box;
use std::env;

#[derive(Debug, Serialize, Deserialize)]
struct BudgetCategory {
    name: String,
    allocated_amount: f64,
    spent_amount: f64,
    transactions: Vec<Transaction>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Transaction {
    date: String,
    description: String,
    amount: f64,
}

#[derive(Deserialize)]
struct Balance {
    currencyCode: String,
    value: String,
}

#[derive(Deserialize)]
struct AccountAttributes {
    displayName: String,
    balance: Balance,
}

#[derive(Deserialize)]
struct Account {
    id: String,
    attributes: AccountAttributes,
}

#[derive(Deserialize)]
struct AccountsResponse {
    data: Vec<Account>,
}

fn get_budget_categories() -> Vec<BudgetCategory> {
    vec![
        BudgetCategory {
            name: "Groceries".to_string(),
            allocated_amount: 500.0,
            spent_amount: 0.0,
            transactions: Vec::new(),
        },
        BudgetCategory {
            name: "Transportation".to_string(),
            allocated_amount: 200.0,
            spent_amount: 0.0,
            transactions: Vec::new(),
        },
        BudgetCategory {
            name: "Entertainment".to_string(),
            allocated_amount: 150.0,
            spent_amount: 0.0,
            transactions: Vec::new(),
        },
        BudgetCategory {
            name: "Utilities".to_string(),
            allocated_amount: 300.0,
            spent_amount: 0.0,
            transactions: Vec::new(),
        },
        BudgetCategory {
            name: "Dining Out".to_string(),
            allocated_amount: 250.0,
            spent_amount: 0.0,
            transactions: Vec::new(),
        },
        // Add more categories as needed
    ]
}

async fn fetch_transactions(api_key: &str) -> Result<Vec<Transaction>, Box<dyn std::error::Error>> {
    let now = Utc::now();
    let current_year = now.year();
    let current_month = now.month();

    // Start date: first day of the current month
    let start_date = format!("{}-{:02}-01T00:00:00Z", current_year, current_month);

    // End date: first day of the next month
    let end_date = if current_month == 12 {
        format!("{}-01-01T00:00:00Z", current_year + 1)
    } else {
        format!("{}-{:02}-01T00:00:00Z", current_year, current_month + 1)
    };

    let client = Client::new();
    let mut transactions = Vec::new();
    let mut next_page_url = Some(format!(
        "https://api.up.com.au/api/v1/transactions?filter[since]={}&filter[until]={}&page[size]=100",
        start_date, end_date
    ));

    while let Some(url) = next_page_url {
        let response = client
            .get(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .send()
            .await?;

        if response.status().is_success() {
            let json: Value = response.json().await?;
            if let Some(data) = json["data"].as_array() {
                for item in data {
                    let amount_str = item["attributes"]["amount"]["value"]
                        .as_str()
                        .unwrap_or("0.00");
                    let amount: f64 = amount_str.parse().unwrap_or(0.0);

                    let transaction = Transaction {
                        date: item["attributes"]["createdAt"]
                            .as_str()
                            .unwrap_or("")
                            .to_string(),
                        description: item["attributes"]["description"]
                            .as_str()
                            .unwrap_or("")
                            .to_string(),
                        amount,
                    };
                    transactions.push(transaction);
                }
                next_page_url = json["links"]["next"].as_str().map(|s| s.to_string());
            } else {
                break;
            }
        } else {
            let error_message = format!(
                "Failed to fetch transactions: {}",
                response.text().await.unwrap_or_default()
            );
            return Err(error_message.into());
        }
    }

    Ok(transactions)
}

fn categorize_transactions(
    transactions: Vec<Transaction>,
    mut budget_categories: Vec<BudgetCategory>,
) -> Vec<BudgetCategory> {
    for transaction in transactions {
        let description_lower = transaction.description.to_lowercase();

        // Match transaction descriptions to categories
        let category = if description_lower.contains("woolworths")
            || description_lower.contains("coles")
            || description_lower.contains("aldi")
        {
            "Groceries"
        } else if description_lower.contains("uber")
            || description_lower.contains("lyft")
            || description_lower.contains("bus")
            || description_lower.contains("train")
        {
            "Transportation"
        } else if description_lower.contains("netflix")
            || description_lower.contains("spotify")
            || description_lower.contains("cinema")
        {
            "Entertainment"
        } else if description_lower.contains("electricity")
            || description_lower.contains("water")
            || description_lower.contains("internet")
            || description_lower.contains("phone")
        {
            "Utilities"
        } else if description_lower.contains("restaurant")
            || description_lower.contains("cafe")
            || description_lower.contains("bar")
            || description_lower.contains("mcdonalds")
            || description_lower.contains("kfc")
        {
            "Dining Out"
        } else {
            "Other"
        };

        // Find the matching budget category and add the transaction
        if let Some(budget_category) = budget_categories.iter_mut().find(|c| c.name == category) {
            budget_category.spent_amount += transaction.amount.abs();
            budget_category.transactions.push(transaction);
        } else {
            // If category not found, add it under "Other"
            if let Some(other_category) = budget_categories.iter_mut().find(|c| c.name == "Other") {
                other_category.spent_amount += transaction.amount.abs();
                other_category.transactions.push(transaction);
            } else {
                // Create "Other" category if it doesn't exist
                budget_categories.push(BudgetCategory {
                    name: "Other".to_string(),
                    allocated_amount: 0.0,
                    spent_amount: transaction.amount.abs(),
                    transactions: vec![transaction],
                });
            }
        }
    }

    budget_categories
}

async fn render_budget_page(budget_categories: Vec<BudgetCategory>) -> HttpResponse {
    let mut categories_html = String::new();

    for category in budget_categories {
        let remaining_amount = category.allocated_amount - category.spent_amount;
        let remaining_class = if remaining_amount >= 0.0 {
            "text-success"
        } else {
            "text-danger"
        };

        let mut transactions_html = String::new();
        for transaction in category.transactions {
            transactions_html.push_str(&format!(
                "<tr>
                    <td>{}</td>
                    <td>{}</td>
                    <td>${:.2}</td>
                </tr>",
                transaction.date, transaction.description, transaction.amount
            ));
        }

        categories_html.push_str(&format!(
            "<div class=\"card mb-4\">
                <div class=\"card-header\">
                    <h4>{}</h4>
                </div>
                <div class=\"card-body\">
                    <p>Allocated Amount: <strong>${:.2}</strong></p>
                    <p>Spent Amount: <strong>${:.2}</strong></p>
                    <p>Remaining Amount: <strong class=\"{}\">${:.2}</strong></p>
                    <button class=\"btn btn-link\" type=\"button\" data-toggle=\"collapse\" data-target=\"#collapse-{}\" aria-expanded=\"false\" aria-controls=\"collapse-{}\">
                        View Transactions
                    </button>
                    <div class=\"collapse\" id=\"collapse-{}\">
                        <div class=\"table-responsive\">
                            <table class=\"table table-striped\">
                                <thead>
                                    <tr>
                                        <th>Date</th>
                                        <th>Description</th>
                                        <th>Amount</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {}
                                </tbody>
                            </table>
                        </div>
                    </div>
                </div>
            </div>",
            category.name,
            category.allocated_amount,
            category.spent_amount,
            remaining_class,
            remaining_amount,
            category.name.replace(" ", "-"),
            category.name.replace(" ", "-"),
            category.name.replace(" ", "-"),
            transactions_html
        ));
    }

    let html_body = format!(
        "<!DOCTYPE html>
        <html lang=\"en\">
        <head>
            <meta charset=\"UTF-8\">
            <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">
            <title>Monthly Budget Overview</title>
            <link rel=\"stylesheet\" href=\"https://stackpath.bootstrapcdn.com/bootstrap/4.5.2/css/bootstrap.min.css\">
            <script src=\"https://code.jquery.com/jquery-3.5.1.slim.min.js\"></script>
            <script src=\"https://cdn.jsdelivr.net/npm/bootstrap@4.5.2/dist/js/bootstrap.bundle.min.js\"></script>
        </head>
        <body>
            <nav class=\"navbar navbar-expand-lg navbar-light bg-light\">
                <a class=\"navbar-brand\" href=\"#\">My Bank App</a>
                <div class=\"collapse navbar-collapse\" id=\"navbarNav\">
                    <ul class=\"navbar-nav\">
                        <li class=\"nav-item\">
                            <a class=\"nav-link\" href=\"/\">Home</a>
                        </li>
                        <li class=\"nav-item active\">
                            <a class=\"nav-link\" href=\"/budget\">Budget <span class=\"sr-only\">(current)</span></a>
                        </li>
                    </ul>
                </div>
            </nav>
            <div class=\"container my-5\">
                <h1 class=\"mb-4\">Monthly Budget Overview</h1>
                {}
            </div>
            <footer class=\"footer mt-auto py-3 bg-light\">
                <div class=\"container\">
                    <span class=\"text-muted\">Powered by My Bank App.</span>
                </div>
            </footer>
        </body>
        </html>",
        categories_html
    );

    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(html_body)
}

async fn budget_page() -> Result<HttpResponse, Error> {
    dotenv().ok();
    let api_key = env::var("API_KEY").expect("UP_BANK_API_KEY must be set");

    let transactions_result = fetch_transactions(&api_key).await;

    match transactions_result {
        Ok(transactions) => {
            let budget_categories = get_budget_categories();
            let categorized_budget = categorize_transactions(transactions, budget_categories);
            Ok(render_budget_page(categorized_budget).await)
        }
        Err(e) => Ok(HttpResponse::InternalServerError()
            .content_type("text/html; charset=utf-8")
            .body(format!("<h1>Error Fetching Transactions</h1><p>{}</p>", e))),
    }
}

async fn landing_page() -> impl Responder {
    let body = r#"
    <!DOCTYPE html>
    <html lang="en">
    <head>
        <meta charset="UTF-8">
        <meta name="viewport" content="width=device-width, initial-scale=1.0">
        <title>Welcome to My Bank App</title>
        <link href="https://stackpath.bootstrapcdn.com/bootstrap/4.5.2/css/bootstrap.min.css" rel="stylesheet">
    </head>
    <body>
        <nav class="navbar navbar-expand-lg navbar-light bg-light">
            <a href="/" class="navbar-brand">My Bank App</a>
        </nav>
        <div class="container text-center">
            <h1 class="my-4">Welcome to Your Bank Dashboard</h1>
            <p class="lead">Manage your accounts with ease.</p>
            <a href="/allbalances" class="btn btn-primary btn-lg">View Balances</a>
            <a href="/expenses" class="btn btn-primary btn-lg">View Expenses</a>
            <a href="/accounts" class="btn btn-primary btn-lg">Select Account</a>
            <a href="/budget" class="btn btn-primary btn-lg">Budget</a>
            <spacer style="height: 100px;"></spacer>
        </div>
        <spacer style="height: 100px;"></spacer>
        <footer class="footer mt-auto py-3 bg-light">
        <spacer style="height: 100px;"></spacer>
            <div class="container">
                <span class="text-muted">Powered by My Bank App.</span>
            </div>
        </footer>
    </body>
    </html>
    "#;

    actix_web::HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(body)
}

async fn list_accounts() -> impl Responder {
    dotenv().ok();
    let api_key = env::var("API_KEY").expect("API_KEY must be set");

    let client = Client::new();
    let response = client
        .get("https://api.up.com.au/api/v1/accounts")
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .expect("Failed to send request");

    let mut buttons = String::new();

    if response.status().is_success() {
        let accounts_response: Value = response.json().await.expect("Failed to parse response");
        if let Some(accounts) = accounts_response["data"].as_array() {
            for account in accounts {
                let display_name = account["attributes"]["displayName"]
                    .as_str()
                    .unwrap_or("Unknown");
                let account_id = account["id"].as_str().unwrap_or("Unknown");

                // Create a button for each account
                buttons.push_str(&format!(
                    "<form action=\"/balances\" method=\"get\" style=\"display: inline-block; margin: 10px;\">
                        <input type=\"hidden\" name=\"account_id\" value=\"{}\">
                        <button type=\"submit\" class=\"btn btn-primary\">{}<br><small>{}</small></button>
                    </form>",
                    account_id, display_name, account_id
                ));
            }
        }
    } else {
        buttons.push_str("<p>Failed to load accounts.</p>");
    }

    let body = format!(
        "<!DOCTYPE html>
        <html lang=\"en\">
        <head>
            <meta charset=\"UTF-8\">
            <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">
            <title>Select Account</title>
            <link href=\"https://stackpath.bootstrapcdn.com/bootstrap/4.5.2/css/bootstrap.min.css\" rel=\"stylesheet\">
        </head>
        <body>
            <nav class=\"navbar navbar-expand-lg navbar-light bg-light\">
                        <a href=\"/\" class=\"navbar-brand\">My Bank App</a>

            </nav>
            <div class=\"container text-center\">
                <h1 class=\"my-4\">Select an Account</h1>
                {}
            </div>
        </body>
        <footer class=\"footer mt-auto py-3 bg-light\">
            <div class=\"container\">
                <span class=\"text-muted\">Powered by My Bank App.</span>
            </div>
        </footer>
        </html>",
        buttons
    );

    actix_web::HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(body)
}

async fn get_balances(req: HttpRequest) -> impl Responder {
    dotenv().ok();
    let api_key = env::var("API_KEY").expect("API_KEY must be set");

    // Extract the account_id from the query parameters
    let account_id = req
        .query_string()
        .split('&')
        .find_map(|pair| {
            let mut iter = pair.split('=');
            if let (Some(key), Some(value)) = (iter.next(), iter.next()) {
                if key == "account_id" {
                    return Some(value);
                }
            }
            None
        })
        .unwrap_or("");

    // Get the current year and month
    let now = Utc::now();
    let current_year = now.year();
    let current_month = now.month();

    // Format the start and end dates with RFC 3339
    let start_date = format!("{}-{:02}-01T00:00:00Z", current_year, current_month);
    let end_date = if current_month == 12 {
        format!("{}-01-01T00:00:00Z", current_year + 1)
    } else {
        format!("{}-{:02}-01T00:00:00Z", current_year, current_month + 1)
    };

    let client = Client::new();
    let mut transactions = Vec::new();
    let mut next_page_url = Some(format!(
        "https://api.up.com.au/api/v1/transactions?filter[since]={}&filter[until]={}&filter[status]=SETTLED&page[size]=100",
        start_date, end_date
    ));

    // Loop to handle pagination
    while let Some(url) = next_page_url {
        let response = client
            .get(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .send()
            .await
            .expect("Failed to send request");

        if response.status().is_success() {
            let json: Value = response.json().await.expect("Failed to parse response");
            if let Some(data) = json["data"].as_array() {
                for transaction in data {
                    // Filter transactions by account_id
                    let transaction_account_id = transaction
                        .get("relationships")
                        .and_then(|rel| rel.get("account"))
                        .and_then(|acc| acc.get("data"))
                        .and_then(|data| data.get("id"))
                        .and_then(|id| id.as_str());

                    if transaction_account_id == Some(account_id) {
                        let description = transaction["attributes"]["description"]
                            .as_str()
                            .unwrap_or("Unknown");
                        let amount = transaction["attributes"]["amount"]["value"]
                            .as_str()
                            .unwrap_or("0.00")
                            .parse::<f64>()
                            .unwrap_or(0.0);
                        let date = transaction["attributes"]["createdAt"]
                            .as_str()
                            .unwrap_or("Unknown");

                        transactions.push(format!(
                            "<li class=\"list-group-item\">{} - {} AUD ({})</li>",
                            date,
                            amount.abs(),
                            description
                        ));
                    }
                }

                // Handle pagination by setting next_page_url to the next link or None if there isn't one
                next_page_url = json["links"]["next"].as_str().map(|s| s.to_string());
            } else {
                break; // No data, exit the loop
            }
        } else {
            break; // Stop on any error response
        }
    }

    let body = format!(
        "<!DOCTYPE html>
        <html lang=\"en\">
        <head>
            <meta charset=\"UTF-8\">
            <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">
            <title>Transactions for Account {}</title>
            <link href=\"https://stackpath.bootstrapcdn.com/bootstrap/4.5.2/css/bootstrap.min.css\" rel=\"stylesheet\">
        </head>
        <body>
            <nav class=\"navbar navbar-expand-lg navbar-light bg-light\">
                <a class=\"navbar-brand\" href=\"#\">My Bank App</a>
            </nav>
            <div class=\"container\">
                <h1 class=\"my-4\">Transactions for Account {}</h1>
                <ul class=\"list-group\">{}</ul>
            </div>
        </body>
        <footer class=\"footer mt-auto py-3 bg-light\">
            <div class=\"container\">
                <span class=\"text-muted\">Powered by My Bank App.</span>
            </div>
        </footer>
        </html>",
        account_id, account_id, transactions.join("")
    );

    actix_web::HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(body)
}

async fn show_balances() -> impl Responder {
    dotenv().ok();
    let api_key = env::var("API_KEY").expect("API_KEY must be set");

    let client = Client::new();
    let response = client
        .get("https://api.up.com.au/api/v1/accounts")
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .expect("Failed to send request");

    let accounts_response: AccountsResponse =
        response.json().await.expect("Failed to parse response");

    let balances: Vec<_> = accounts_response
        .data
        .iter()
        .map(|account| {
            format!(
                "<li class=\"list-group-item\">Account: {}, Balance: {} {}</li>",
                account.attributes.displayName,
                account.attributes.balance.value,
                account.attributes.balance.currencyCode
            )
        })
        .collect();

    let body = format!(
        "<!DOCTYPE html>
        <html lang=\"en\">
            <nav class=\"navbar navbar-expand-lg navbar-light bg-light\">
    <a class=\"navbar-brand\" href=\"#\">My Bank App</a>
</nav>
        <head>
            <meta charset=\"UTF-8\">
            <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">
            <title>Account Balances</title>
            <link href=\"https://stackpath.bootstrapcdn.com/bootstrap/4.5.2/css/bootstrap.min.css\" rel=\"stylesheet\">
        </head>
        <body>
            <div class=\"container\">
                <h1 class=\"my-4\">Your Account Balances</h1>
                <ul class=\"list-group\">{}</ul>
            </div>
        </body>
        <footer class=\"footer mt-auto py-3 bg-light\">
    <div class=\"container\">
        <span class=\"text-muted\">Place sticky footer content here.</span>
    </div>
</footer>
        </html>",
        balances.join("")
    );

    actix_web::HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(body)
}

async fn get_expenses() -> impl Responder {
    dotenv().ok();
    let api_key = env::var("API_KEY").expect("UP_BANK_API_KEY must be set");

    // Get the current year and month
    let now = Utc::now();
    let current_year = now.year();
    let current_month = now.month();

    // Format the start and end dates with RFC 3339
    let start_date = format!("{}-{:02}-01T00:00:00Z", current_year, current_month);
    let end_date = if current_month == 12 {
        format!("{}-01-01T00:00:00Z", current_year + 1)
    } else {
        format!("{}-{:02}-01T00:00:00Z", current_year, current_month + 1)
    };

    let client = Client::new();
    let mut transactions = Vec::new();
    let mut total_expenses = 0.0;
    let mut total_incoming = 0.0;
    let mut next_page_url = Some(format!(
        "https://api.up.com.au/api/v1/transactions?filter[since]={}&filter[until]={}&filter[status]=SETTLED&page[size]=100",
        start_date, end_date
    ));

    // Loop to handle pagination
    while let Some(url) = next_page_url {
        let response = client
            .get(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .send()
            .await
            .expect("Failed to send request");

        if response.status().is_success() {
            let json: Value = response.json().await.expect("Failed to parse response");
            if let Some(data) = json["data"].as_array() {
                for transaction in data {
                    let description = transaction["attributes"]["description"]
                        .as_str()
                        .unwrap_or("Unknown");
                    let amount = transaction["attributes"]["amount"]["value"]
                        .as_str()
                        .unwrap_or("0.00")
                        .parse::<f64>()
                        .unwrap_or(0.0);
                    let date = transaction["attributes"]["createdAt"]
                        .as_str()
                        .unwrap_or("Unknown");

                    // Track total expenses and incoming money
                    if amount < 0.0 {
                        total_expenses += amount.abs(); // Expenses are typically negative amounts
                    } else {
                        total_incoming += amount; // Positive amounts are incoming money
                    }

                    // Double-entry: Debit the expense (assume "Expenses" as a placeholder) and Credit the Spending account
                    transactions.push(format!(
                        "<li class=\"list-group-item\">{} - Debit: Expenses {:.2} AUD, Credit: Account {:.2} AUD</li>",
                        date, amount.abs(), amount.abs()
                    ));
                }

                // Handle pagination by setting next_page_url to the next link or None if there isn't one
                next_page_url = json["links"]["next"].as_str().map(|s| s.to_string());
            } else {
                break; // No data, exit the loop
            }
        } else {
            break; // Stop on any error response
        }
    }

    let body = format!(
    "<!DOCTYPE html>
    <html lang=\"en\">
    <head>
        <meta charset=\"UTF-8\">
        <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">
        <title>Expenses for Current Month</title>
        <link href=\"https://stackpath.bootstrapcdn.com/bootstrap/4.5.2/css/bootstrap.min.css\" rel=\"stylesheet\">
        <style>
            .negative {{ color: red; }}
        </style>
    </head>
    <body>
        <nav class=\"navbar navbar-expand-lg navbar-light bg-light\">
            <a class=\"navbar-brand\" href=\"\\\">My Bank App</a>
        </nav>
        <div class=\"container\">
            <h1 class=\"my-4\">Expenses for {}/{} </h1>
            <h3>Total Expenses: <span class=\"{}\">{:.2} AUD    Total Incoming Money: {:.2} AUD</span></h3>
        <h3>Change in position: {:.2} AUD</h3>
            <ul class=\"list-group\">{}</ul>
        </div>
    </body>
    <footer class=\"footer mt-auto py-3 bg-light\">
        <div class=\"container\">
            <span class=\"text-muted\">Powered by My Bank App.</span>
        </div>
    </footer>
    </html>",
    current_month,
    current_year,
    if total_expenses > 0.0 { "" } else { "negative" }, // Apply "negative" class if expenses are negative
    total_expenses*-1.0,
    total_incoming,
    total_incoming - total_expenses,
    transactions.join("")
);

    actix_web::HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(body)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new()
            .route("/", web::get().to(landing_page))
            .route("/allbalances", web::get().to(show_balances))
            .route("/balances", web::get().to(get_balances))
            .route("/expenses", web::get().to(get_expenses))
            .route("/accounts", web::get().to(list_accounts))
            .service(web::resource("/budget").route(web::get().to(budget_page)))
            .service(actix_files::Files::new("/static", "static").show_files_listing())
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
