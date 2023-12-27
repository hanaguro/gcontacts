// Copyright 2023 Takahiro Yoshizawa
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

// 必要なクレートとモジュールをインポートする
use hyper::client::Client; // HTTPクライアント操作用
use hyper_rustls::HttpsConnector; // HTTPSサポート用
use google_people1::{PeopleService, FieldMask}; // Google People APIを使用するため
use std::str::FromStr; // 文字列を型に変換するため
use csv::WriterBuilder; // CSVファイルを書き込むため
use std::io; // 入出力機能のための 'io' モジュールをインポート
use std::path::Path; // ファイルパスを扱うための 'Path' モジュールをインポート
use std::env; // 環境変数を扱うための 'env' モジュールをインポート
use fluent::{bundle::FluentBundle, FluentResource}; // ローカライゼーション機能を提供するfluentクレート関連モジュール
use intl_memoizer::concurrent::IntlLangMemoizer; // 国際化機能を提供するintl_memoizerクレートのモジュール

mod mod_locale;
mod mod_fluent;
mod mod_auth;

/// アプリケーションのヘルプメッセージを表示する関数。
///
/// この関数は、アプリケーションの一般的な説明を出力します。FluentBundleを利用して、
/// 国際化されたアプリケーションの説明を取得し、表示します。FluentBundleによって指定された
/// 言語でアプリケーションの説明をフェッチし、表示します。
///
/// # 引数
/// * `bundle` - ローカライズされた文字列と国際化の詳細を含むFluentBundleへの参照。
fn print_help(bundle: &FluentBundle<FluentResource, IntlLangMemoizer>) {
    // アプリケーションの全体的な説明を表示
    println!("Application Description:");
    // Fluentバンドルを使用して、アプリケーションの説明を国際化対応の言語で取得し表示
    println!("\t{}", mod_fluent::get_translation(bundle, "app-description"));
    // 追加の詳細説明や使用方法のためのプレースホルダー
    // ここに他の詳細な説明や使用方法を記述する
}

/// 文字列内で最初に数字が現れる部分を見つけ、文字列部分と数値部分に分割する。
///
/// # 引数
/// * `s` - 分割対象の文字列。
///
/// # 戻り値
/// 文字列部分と数値部分を含むタプル。数値部分が存在しない場合は0を返す。
fn split_string_and_number(s: &str) -> (String, u32) {
    // 数字が始まるインデックスを保存するための変数
    let mut num_start_index = None;

    // 文字列の各文字に対してループを行う
    for (index, character) in s.char_indices() {
        // 文字が数字かどうかをチェック
        if character.is_digit(10) {
            // 数字が見つかった場合、そのインデックスを保存しループを抜ける
            num_start_index = Some(index);
            break;
        }
    }

    // 数字が見つかった場合の処理
    match num_start_index {
        // 数字が見つかった場合
        Some(index) => {
            // 文字列を数字が始まる箇所で分割する
            let string_part = &s[..index];
            let number_part = &s[index..];
            // 文字列部分と数値部分をタプルとして返す
            (string_part.to_string(), number_part.parse().unwrap_or(0))
        },
        // 数字が見つからなかった場合
        None => (s.to_string(), 0),
    }
}

/// 与えられた名前とメールアドレスの数に基づいてニックネームを生成する。
///
/// # 引数
/// * `name` - 名前の文字列。
/// * `email_count` - メールアドレスの数。
/// * `existing_nicknames` - 既に存在するニックネームのリストへの可変参照。
///
/// # 戻り値
/// 生成されたニックネームの文字列。
fn generate_nickname(name: &str, email_count: usize, existing_nicknames: &mut Vec<String>) -> String {
    // 名前の最後の部分を取得し、基本的なニックネームを作成
    let last_name_part = name.split_whitespace().last().unwrap_or("Unknown").to_string();
    let base_nickname;
    let mut counter;

    // 既に存在するニックネームがある場合は、その最初のニックネームから数値部分を抽出
    if !existing_nicknames.is_empty(){
        (base_nickname, counter) = split_string_and_number(existing_nicknames[0].as_str());
    } else {
        // 既存のニックネームがない場合は、名前の最後の部分を基本ニックネームとして使用
        base_nickname = last_name_part;
        counter = 0;
    }

    // メールアドレスが複数ある場合、一意のニックネームを生成
    if email_count > 1 {
        // 基本ニックネームに数値を追加してニックネームを生成
        let mut nickname = format!("{}{:02}", base_nickname, counter + 1);
        // 既存のニックネームと重複しないようにカウンターを増やしながらニックネームを生成
        while existing_nicknames.contains(&nickname) {
            counter += 1;
            nickname = format!("{}{:02}", base_nickname, counter + 1);
        }
        // 生成されたニックネームをリストに追加
        existing_nicknames.push(nickname.clone());
        nickname
    } else {
        // メールアドレスが1つのみの場合は基本ニックネームを使用
        existing_nicknames.push(base_nickname.clone());
        base_nickname
    }
}

// 非同期のメイン関数
#[tokio::main]
async fn main(){
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
        std::process::exit(1);
    }

    // 認証が成功した場合の処理を続行
    let auth = match mod_auth::get_auth().await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{}: {}", mod_fluent::get_translation(&bundle, "auth-error"), e);
            std::process::exit(1);
        }
    };

    // PeopleService（Google People APIクライアント）を初期化
    let service = PeopleService::new(Client::builder().build(HttpsConnector::with_native_roots()), auth);

    // Google People APIから取得するフィールドを設定
    let field_mask = FieldMask::from_str("nicknames,names,emailAddresses,biographies").unwrap(); // 失敗したらパニック

    // Google People APIを使用して連絡先情報を取得
    // resultsは(Response<Body>, ListConnectionsResponse)のタプル
    let results = service.people().connections_list("people/me")
        .page_size(1000)
        .person_fields(field_mask)
        .doit().await.unwrap_or_else(|e|{
            eprintln!("{}: {}", mod_fluent::get_translation(&bundle, "fail-contact"), e);
            std::process::exit(1);
        });

    // CSVファイルの保存場所を指定
    let home_dir = dirs::home_dir().unwrap_or_else(|| {
        eprintln!("{}",mod_fluent::get_translation(&bundle, "home-notfound"));
        std::process::exit(1);
    });

    let addressbook_path = home_dir.join(".addressbook");

    // ファイルが存在するかチェックし、存在する場合は上書き確認する
    if Path::new(&addressbook_path).exists() {
        println!("{}",mod_fluent::get_translation(&bundle, "overwrite-or-not"));
        let mut input = String::new();
        if let Err(e) = io::stdin().read_line(&mut input) {
            eprintln!("{}: {}",mod_fluent::get_translation(&bundle, "input-error"), e);
            std::process::exit(1);
        };

        if input.trim().to_lowercase() != "y" {
            println!("{}", mod_fluent::get_translation(&bundle, "op-cancel"));
            std::process::exit(1);
        }
    }

    // CSVファイルライター（タブ区切り）を初期化
    let mut writer = WriterBuilder::new()
        .delimiter(b'\t')
        .from_path(addressbook_path).unwrap_or_else(|e| {
            eprintln!("{}: {}", mod_fluent::get_translation(&bundle, "init-error"), e);
            std::process::exit(1);
        });

    // 取得した連絡先情報に基づいて処理
    if let Some(connections) = results.1.connections {
        for person in connections {
            // 生成されたニックネームを格納するVec
            let mut existing_nicknames = Vec::new();

            // Google Contactsから各人物のニックネームと名前とメールアドレスを取得
            let nicknames = person.nicknames.unwrap_or_else(Vec::new);
            let names = person.names.unwrap_or_else(Vec::new);
            let emails = person.email_addresses.unwrap_or_else(Vec::new);
            let biographies = person.biographies.unwrap_or_else(Vec::new);

            // 名前が存在する場合のみ処理
            if !names.is_empty() {
                if !nicknames.is_empty(){
                    // Google Contactsに登録されている最初のニックネームを取得する
                    let nickname_from_g = nicknames[0].value.as_ref().map(|s| s.as_str()).unwrap_or("");
                    // nickname_from_gが空文字の場合はニックネームとみなさない
                    if !nickname_from_g.is_empty() {
                        existing_nicknames.push(nickname_from_g.to_string());
                    }
                }

                // 名前を取得する
                let name;
                if !names.is_empty() {
                    name = names[0].display_name.as_ref().map(|s| s.as_str()).unwrap_or("default");
                }else{
                    name = "";
                }

                // メモ欄の内容を取得する
                let memo;
                if !biographies.is_empty() {
                    memo = biographies[0].value.as_ref().map(|s| s.as_str()).unwrap_or("");
                }else{
                    memo = "";
                }

                let email_count = emails.len();

                // 各メールアドレスにニックネームを割り当ててCSVに書き込む
                for email in emails {
                    let email_address = email.value.unwrap_or_default();
                    let nickname = generate_nickname(&name, email_count, &mut existing_nicknames);
                    if let Err(e) = writer.write_record(&[&nickname, name, &email_address, "", memo]) {
                       eprintln!("{}: {}", mod_fluent::get_translation(&bundle, "write-error"), e);
                       std::process::exit(1);
                    }
                }
            }
        }
    };

    // CSVファイルへの書き込みを完了
    if let Err(e) = writer.flush() {
        eprintln!("{}: {}", mod_fluent::get_translation(&bundle, "flush-error"), e);
        std::process::exit(1);
    };


    // 書き込み完了メッセージを表示
    println!("{}", mod_fluent::get_translation(&bundle, "export-complete"));
}
