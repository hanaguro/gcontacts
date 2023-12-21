// Copyright 2023, Takahiro Yoshizawa
// Use of this source code is permitted under the license.
// The license can be viewed in the LICENSE file located in the project's top directory.

// Author: Takahiro Yoshizawa
// Description: A Rust program to process contact information using Google People API
// and export it to the AddressBook of Alpine Email Program.

// 必要なクレートとモジュールをインポートする
use hyper::client::Client; // HTTPクライアント操作用
use hyper_rustls::HttpsConnector; // HTTPSサポート用
use std::str::FromStr; // 文字列を型に変換するため
use google_people1::{PeopleService, FieldMask}; // Google People APIを使用するため
use std::collections::HashSet; // データに対する集合演算用
use csv::WriterBuilder; // CSVファイルを書き込むため
use std::io; // 入出力機能のための 'io' モジュールをインポート
use std::path::Path; // ファイルパスを扱うための 'Path' モジュールをインポート
use std::env; // 環境変数を扱うための 'env' モジュールをインポート
use fluent::{bundle::FluentBundle, FluentResource}; // ローカライゼーション機能を提供するfluentクレート関連モジュール
use intl_memoizer::concurrent::IntlLangMemoizer; // 国際化機能を提供するintl_memoizerクレートのモジュール

mod mod_locale;
mod mod_fluent;
mod mod_auth;

// アプリケーションのヘルプメッセージを表示する関数
fn print_help(bundle: &FluentBundle<FluentResource, IntlLangMemoizer>) {
    // アプリケーションの全体的な説明を表示
    println!("Application Description:");
    // Fluentバンドルを使用して、アプリケーションの説明を国際化対応の言語で取得し表示
    println!("\t{}", mod_fluent::get_translation(bundle, "app-description"));
    // 追加の詳細説明や使用方法のためのプレースホルダー
    // ここに他の詳細な説明や使用方法を記述する
}

// 与えられた名前とメールアドレスの数に基づいてニックネームを生成する関数
fn generate_nickname(name: &str, email_count: usize, existing_nicknames: &mut HashSet<String>) -> String {
    // 名前の最後の部分（姓と仮定）を取得し、基本的なニックネームを作成
    let last_name_part = name.split_whitespace().last().unwrap_or("Unknown").to_string();
    let base_nickname = last_name_part;

    // 複数のメールアドレスがある場合、一意のニックネームを生成
    if email_count > 1 {
        let mut counter = 1;
        let mut nickname = format!("{}{:02}", base_nickname, counter);
        // 既に存在するニックネームと重複しないようにカウンターを増やしながらニックネームを生成
        while existing_nicknames.contains(&nickname) {
            counter += 1;
            nickname = format!("{}{:02}", base_nickname, counter);
        }
        existing_nicknames.insert(nickname.clone());
        nickname
    } else {
        // メールアドレスが1つだけの場合は基本的なニックネームを使用
        existing_nicknames.insert(base_nickname.clone());
        base_nickname
    }
}

// 非同期のメイン関数
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ロケールの設定（コマンドライン引数、環境変数、既定値などから）
    // LANG環境変数からロケールを取得する
    let locale = mod_locale::get_locale_from_env();
    // Fluentバンドルを初期化
    let bundle = mod_fluent::init_fluent_bundle(&locale);

    // コマンドライン引数を取得
    let args: Vec<String> = env::args().collect();

    // --help オプションのチェック
    if args.contains(&"--help".to_string()) {
        // ヘルプメッセージを表示
        print_help(&bundle);
        return Ok(());
    }

    // 認証が成功した場合の処理を続行
    let auth = match mod_auth::get_auth().await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{}: {}", mod_fluent::get_translation(&bundle, "auth-error"), e);
            Err(e)
        }?
    };

    // PeopleService（Google People APIクライアント）を初期化
    let service = PeopleService::new(Client::builder().build(HttpsConnector::with_native_roots()), auth);

    // Google People APIから取得するフィールドを設定
    let field_mask = match FieldMask::from_str("names,emailAddresses") {
        Ok(f) => f,
        Err(e) => {
            eprintln!("{}: {}", mod_fluent::get_translation(&bundle, "field-error"), e);
            Err(Box::new(e) as Box<dyn std::error::Error>)
        }?
    };

    // Google People APIを使用して連絡先情報を取得
    // resultsは(Response<Body>, ListConnectionsResponse)のタプル
    let results = match service.people().connections_list("people/me")
        .page_size(1000)
        .person_fields(field_mask)
        .doit().await {
            Ok(r) => r,
            Err(e) => {
                eprintln!("{}: {}", mod_fluent::get_translation(&bundle, "fail-contact"), e);
                Err(Box::new(e) as Box<dyn std::error::Error>)
            }?
        };

    // 生成されたニックネームを格納するHashSet
    let mut existing_nicknames = HashSet::new();
    // CSVファイルの保存場所を指定
    let home_dir = dirs::home_dir().ok_or_else(|| {
        eprintln!("{}",mod_fluent::get_translation(&bundle, "home-notfound"));
        Box::new(std::io::Error::new(std::io::ErrorKind::NotFound, mod_fluent::get_translation(&bundle, "home-notfound"))) as Box<dyn std::error::Error>
    })?;

    let addressbook_path = home_dir.join(".addressbook");

    // ファイルが存在するかチェック
    if Path::new(&addressbook_path).exists() {
        println!("{}",mod_fluent::get_translation(&bundle, "overwrite-or-not"));
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        if input.trim().to_lowercase() != "y" {
            println!("{}", mod_fluent::get_translation(&bundle, "op-cancel"));
            return Ok(());
        }
    }

    // CSVファイルライター（タブ区切り）を初期化
    let mut writer = match WriterBuilder::new()
        .delimiter(b'\t')
        .from_path(addressbook_path) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("{}: {}", mod_fluent::get_translation(&bundle, "init-error"), e);
                Err(Box::new(e) as Box<dyn std::error::Error>)
            }?
    };

    // 取得した連絡先情報に基づいて処理
    if let Some(connections) = results.1.connections {
        for person in connections {
            // 各人物の名前とメールアドレスを取得
            let names = person.names.unwrap_or_else(Vec::new);
            let emails = person.email_addresses.unwrap_or_else(Vec::new);

            // 名前が存在する場合のみ処理
            if !names.is_empty() {
                let name = names[0].display_name.as_ref().map(|s| s.as_str()).unwrap_or("default");
                let email_count = emails.len();

                // 各メールアドレスにニックネームを割り当ててCSVに書き込む
                for email in emails {
                    let email_address = email.value.unwrap_or_default();
                    let nickname = generate_nickname(&name, email_count, &mut existing_nicknames);
                    writer.write_record(&[&nickname, name, &email_address])
                        .map_err(|e| {
                            eprintln!("{}: {}", mod_fluent::get_translation(&bundle, "write-error"), e);
                            e
                        })?;
                }
            }
        }
    };

    // CSVファイルへの書き込みを完了
    writer.flush().map_err(|e| {
        eprintln!("{}: {}", mod_fluent::get_translation(&bundle, "flush-error"), e);
        e
    })?;

    // 書き込み完了メッセージを表示
    println!("{}", mod_fluent::get_translation(&bundle, "export-complete"));
    Ok(())
}
