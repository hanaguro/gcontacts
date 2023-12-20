// Copyright 2023, Takahiro Yoshizawa
// Use of this source code is permitted under the license.
// The license can be viewed in the LICENSE file located in the project's top directory.

// Author: Takahiro Yoshizawa
// Description: A Rust program to process contact information using Google People API
// and export it to the AddressBook of Alpine Email Program.

// 必要なクレートとモジュールをインポートする
use yup_oauth2::{InstalledFlowAuthenticator, InstalledFlowReturnMethod, read_application_secret, authenticator::Authenticator}; // OAuth2認証のためのモジュール
use hyper::client::{Client, HttpConnector}; // HTTPクライアント操作用
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
use std::fs; // ファイルシステム操作のための標準ライブラリのモジュール


fn get_locale_from_env() -> String {
    // 環境変数 'LANG' からロケール設定を取得する
    if let Ok(lang) = env::var("LANG") {
        // もし 'LANG' が "C" か空だった場合、デフォルトの "en-US" を返す
        if lang == "C" || lang.is_empty() {
            "en-US".to_string()
        } else {
            // 'LANG' の値からロケールコードを抽出する
            let lang_code = lang.split('.').next().unwrap_or("");
            let lang_code = lang_code.replace("_", "-");

            // ロケールコードが一般的な形式に合致しているかチェック
            if is_valid_locale_format(&lang_code) {
                // 有効なロケール形式なら、そのコードを返す
                lang_code
            } else {
                // 無効な形式の場合、デフォルトの "en-US" を返す
                "en-US".to_string()
            }
        }
    } else {
        // 'LANG' 環境変数が設定されていない場合、"en-US" を返す
        "en-US".to_string()
    }
}

// ロケールコードの形式が有効かどうかをチェックするヘルパー関数
fn is_valid_locale_format(code: &str) -> bool {
    // ロケールコードを '-' で分割して部分文字列のベクトルを生成
    let parts: Vec<&str> = code.split('-').collect();
    // ロケールコードが2つの部分から成り、各部分が英数字のみで構成されているかをチェック
    parts.len() == 2 && parts.iter().all(|&part| part.chars().all(|c| c.is_alphanumeric()))
}

// intl_memoizer::concurrent::IntlLangMemoizerを型引数として指定
fn init_fluent_bundle(locale: &str) -> FluentBundle<FluentResource, IntlLangMemoizer> {
    // 指定されたロケールに対応するFTLファイルのパスを構築
    let ftl_path = format!("locales/{}.ftl", locale);
    // FTLファイルを文字列として読み込む
    let ftl_string = match fs::read_to_string(&ftl_path) {
        Ok(s) => s,
        Err(_) => {
            // 指定されたロケールのファイルが存在しない場合、デフォルトのロケールを使用
            let default_ftl_path = "locales/en-US.ftl";
            fs::read_to_string(default_ftl_path)
                .expect("Default FTL file not found")
        }
    };
    // Fluentリソースを生成し、エラーがあればパニック
    let resource = FluentResource::try_new(ftl_string).expect("Failed to parse an FTL string.");

    // FluentBundleを並行処理対応で新規作成
    let mut bundle = FluentBundle::new_concurrent(vec![locale.parse().expect("Failed to parse.")]);
    // リソースをバンドルに追加し、エラーがあればパニック
    bundle.add_resource(resource).expect("Failed to add FTL resource to the bundle");

    // 完成したバンドルを返す
    bundle
}


// 翻訳メッセージを取得する関数
fn get_translation(bundle: &FluentBundle<FluentResource, IntlLangMemoizer>, message_id: &str) -> String {
    let message = bundle.get_message(message_id).expect("Message doesn't exist.");
    let pattern = message.value().expect("Message has no value.");
    let mut errors = vec![];
    bundle.format_pattern(&pattern, None, &mut errors).to_string()
}

// アプリケーションのヘルプメッセージを表示する関数
fn print_help(bundle: &FluentBundle<FluentResource, IntlLangMemoizer>) {
    // アプリケーションの全体的な説明を表示
    println!("Application Description:");
    // Fluentバンドルを使用して、アプリケーションの説明を国際化対応の言語で取得し表示
    println!("\t{}", get_translation(bundle, "app-description"));
    // 追加の詳細説明や使用方法のためのプレースホルダー
    // ここに他の詳細な説明や使用方法を記述する
}


// Google APIとの認証を行いAuthenticatorを返す非同期関数
async fn get_auth() -> Result<Authenticator<HttpsConnector<HttpConnector>>, Box<dyn std::error::Error>> {
    // ユーザーのホームディレクトリを取得
    let home_dir = dirs::home_dir().expect("Home directory not found");
    // Rustプロジェクトの名前を動的に取得
    let project_name = env!("CARGO_PKG_NAME");
    // プロジェクトのディレクトリパスを作成
    let project_dir = home_dir.join(format!(".{}", project_name));

    // プロジェクトディレクトリが存在するかチェックし、存在しない場合は作成する
    if !project_dir.exists() {
        std::fs::create_dir(&project_dir)?;
    }

    // 認証情報とトークンキャッシュのファイルパスを設定
    let secret_file = project_dir.join("client_secret.json");
    let token_cache_file = project_dir.join("token_cache.json");

    // `secret_file` のパスをクローンし`secret_file_path`に保存
    // これにより、所有権が移された後もファイルパスを使用できる
    let secret_file_path = secret_file.clone();

    // ファイルからGoogle API認証情報を読み込む
    // `read_application_secret` 関数は非同期で実行され、`ApplicationSecret`型の結果を返す
    let secret = match read_application_secret(secret_file).await {
        // 認証情報の読み込みが成功した場合、結果を`secret`に保存
        Ok(s) => s,
        // 読み込み失敗の場合、エラーメッセージを表示してエラーを返す
        Err(e) => {
           eprintln!("Failed to open {}: {}", secret_file_path.display(), e);
           Err(e)
        }?
        // `?` 演算子は`Result`型から`Ok`の値を抽出し、`Err`の場合は呼び出し元の関数にエラーを返す
    };

    // HTTPS対応のHTTPクライアントを構築
    let client = Client::builder().build(HttpsConnector::with_native_roots());

    // OAuth2認証フローを構築して返す
    let auth = InstalledFlowAuthenticator::builder(secret, InstalledFlowReturnMethod::HTTPRedirect)
        .persist_tokens_to_disk(token_cache_file)
        .hyper_client(client)
        .build()
        .await?;

    Ok(auth)
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
    let locale = get_locale_from_env();
    // Fluentバンドルを初期化
    let bundle = init_fluent_bundle(&locale);

    // コマンドライン引数を取得
    let args: Vec<String> = env::args().collect();

    // --help オプションのチェック
    if args.contains(&"--help".to_string()) {
        // ヘルプメッセージを表示
        print_help(&bundle);
        return Ok(());
    }

    // 認証が成功した場合の処理を続行
    let auth = match get_auth().await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{}: {}", get_translation(&bundle, "auth-error"), e);
            Err(e)
        }?
    };

    // PeopleService（Google People APIクライアント）を初期化
    let service = PeopleService::new(Client::builder().build(HttpsConnector::with_native_roots()), auth);

    // Google People APIから取得するフィールドを設定
    let field_mask = match FieldMask::from_str("names,emailAddresses") {
        Ok(f) => f,
        Err(e) => {
            eprintln!("{}: {}", get_translation(&bundle, "field-error"), e);
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
                eprintln!("{}: {}", get_translation(&bundle, "fail-contact"), e);
                Err(Box::new(e) as Box<dyn std::error::Error>)
            }?
        };

    // 生成されたニックネームを格納するHashSet
    let mut existing_nicknames = HashSet::new();
    // CSVファイルの保存場所を指定
    let home_dir = dirs::home_dir().ok_or_else(|| {
        eprintln!("{}",get_translation(&bundle, "home-notfound"));
        Box::new(std::io::Error::new(std::io::ErrorKind::NotFound, get_translation(&bundle, "home-notfound"))) as Box<dyn std::error::Error>
    })?;

    let addressbook_path = home_dir.join(".addressbook");

    // ファイルが存在するかチェック
    if Path::new(&addressbook_path).exists() {
        println!("{}",get_translation(&bundle, "overwrite-or-not"));
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        if input.trim().to_lowercase() != "y" {
            println!("{}", get_translation(&bundle, "op-cancel"));
            return Ok(());
        }
    }

    // CSVファイルライター（タブ区切り）を初期化
    let mut writer = match WriterBuilder::new()
        .delimiter(b'\t')
        .from_path(addressbook_path) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("{}: {}", get_translation(&bundle, "init-error"), e);
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
                            eprintln!("{}: {}", get_translation(&bundle, "write-error"), e);
                            e
                        })?;
                }
            }
        }
    };

    // CSVファイルへの書き込みを完了
    writer.flush().map_err(|e| {
        eprintln!("{}: {}", get_translation(&bundle, "flush-error"), e);
        e
    })?;

    // 書き込み完了メッセージを表示
    println!("{}", get_translation(&bundle, "export-complete"));
    Ok(())
}
