// 著作権 2023, Takahiro Yoshizawa
// このソースコードの使用は、ライセンスに従って許可されています。
// ライセンスは、プロジェクトのトップディレクトリにあるLICENSEファイルで見ることができます。

// 作者: Takahiro Yoshizawa
// 説明: Google People APIを用いて連絡先情報を処理し、
// Alpine Email ProgramのAddressBookにエクスポートするRustプログラム。

// 必要なクレートとモジュールをインポート
use yup_oauth2::{read_application_secret, authenticator::Authenticator}; // OAuth2認証用のモジュール
use hyper::client::{Client, HttpConnector}; // HTTPクライアント操作用
use hyper_rustls::HttpsConnector; // HTTPSサポート用
use std::str::FromStr; // 文字列からの型変換サポート用
use google_people1::{PeopleService, oauth2, FieldMask}; // Google People APIの利用
use std::collections::HashSet; // データの集合操作用
use csv::WriterBuilder; // CSVファイル書き込み用
use std::io;
use std::path::Path;
use std::env;

fn print_help() {
    println!("アプリケーションの説明:");
    println!("\tこのアプリケーションは、Google People APIを用いて連絡先情報を取得し、");
    println!("\t~/.addressbookにAlpine Email ProgramのAddressBook形式でエクスポートします。");
    // その他の詳細な説明や使用方法をここに記述
}

// Google APIのOAuth2認証を行いAuthenticatorを返す非同期関数
async fn get_auth() -> Result<Authenticator<HttpsConnector<HttpConnector>>, Box<dyn std::error::Error>> {
    // ユーザーのホームディレクトリを取得
    let home_dir = dirs::home_dir().expect("ホームディレクトリが見つかりません");
    // 認証情報とトークンキャッシュのファイルパスを設定
    let secret_file = home_dir.join(".client_secret.json");
    let token_cache_file = home_dir.join(".token_cache.json");

    // Google APIの認証情報をファイルから読み込み
    let secret = read_application_secret(secret_file).await?;

    // HTTPS対応のHTTPクライアントを構築
    let client = Client::builder().build(HttpsConnector::with_native_roots());

    // OAuth2認証フローを構築して返す
    let auth = oauth2::InstalledFlowAuthenticator::builder(secret, oauth2::InstalledFlowReturnMethod::HTTPRedirect)
        .persist_tokens_to_disk(token_cache_file)
        .hyper_client(client)
        .build()
        .await?;

    Ok(auth)
}

// 与えられた名前とメールアドレスの数に基づきニックネームを生成する関数
fn generate_nickname(name: &str, email_count: usize, existing_nicknames: &mut HashSet<String>) -> String {
    // 名前の最後の部分（姓を想定）を取得し、それをベースとするニックネームを作成
	let last_name_part = name.split_whitespace().last().unwrap_or("Unknown").to_string();
	let base_nickname = last_name_part;

    // メールアドレスが複数ある場合はユニークなニックネームを生成
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
        // メールアドレスが1つだけの場合は、ベースのニックネームを使用
		existing_nicknames.insert(base_nickname.clone());
		base_nickname
	}
}

// メイン関数（非同期）
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // コマンドライン引数を取得
    let args: Vec<String> = env::args().collect();

    // --help オプションのチェック
    if args.contains(&"--help".to_string()) {
        print_help();
        return Ok(());
    }

    // 認証を行い、成功すれば処理を続行
    let auth = match get_auth().await{
        Ok(a) => a,
        Err(e) => {
            eprintln!("認証に失敗しました: {}", e);
            Err(e)
        }?
    };    

    // PeopleService（Google People APIクライアント）の初期化
    let service = PeopleService::new(Client::builder().build(HttpsConnector::with_native_roots()), auth);
 
    // Google People APIで取得するフィールドを設定
    let field_mask = match FieldMask::from_str("names,emailAddresses") {
        Ok(f) => f,
        Err(e) => {
            eprintln!("フィールドが取得できません: {}", e);
            Err(Box::new(e) as Box<dyn std::error::Error>)
        }?
    };
 
    // Google People APIを使って連絡先情報を取得
    // resultsはタプル(Response<Body>, ListConnectionsResponse)
    // doit()の戻り値はResult<(Response<Body>, ListConnectionsResponse)>
    let results = match service.people().connections_list("people/me") 
       .page_size(1000)
       .person_fields(field_mask)
       .doit().await {
           Ok(r) => r,
           Err(e) => {
                eprintln!("連絡先情報を取得できませんでした: {}", e);
	            Err(Box::new(e) as Box<dyn std::error::Error>)
           }?
		};

    // 生成されたニックネームを格納するHashSet
    let mut existing_nicknames = HashSet::new();
    // CSVファイルの保存場所を指定
    let home_dir = dirs::home_dir().ok_or_else(|| {
        // このブロックはクロージャです。ここでエラーを生成しています
        eprintln!("ホームディレクトリが見つかりません");
        Box::new(std::io::Error::new(std::io::ErrorKind::NotFound, "ホームディレクトリが見つかりません")) as Box<dyn std::error::Error>
    })?;
 
    let addressbook_path = home_dir.join(".addressbook");

    // ファイルが存在するか確認
    if Path::new(&addressbook_path).exists() {
        println!("ファイルが存在します。上書きしますか？ [y/N]");
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        if input.trim().to_lowercase() != "y" {
            println!("操作をキャンセルしました。");
            return Ok(());
        }
    }

    // CSVファイルライターの初期化（タブ区切り）
	let mut writer = match WriterBuilder::new()
	    .delimiter(b'\t')
	    .from_path(addressbook_path) {
	        Ok(w) => w,
	        Err(e) => {
	            eprintln!("アドレス帳の初期化に失敗しました: {}", e);
	            // ここで処理を終了するか、またはエラーを上位に伝播させる
	            Err(Box::new(e) as Box<dyn std::error::Error>)
	        }?
	};

    // 取得した連絡先情報に基づいて処理
    // results.1はListConnectionsResponse型
    if let Some(connections) = results.1.connections {
        for person in connections {
            // 各人物の名前とメールアドレスを取得
            let names = person.names.unwrap_or_else(Vec::new);
            let emails = person.email_addresses.unwrap_or_else(Vec::new);
 
            // 名前がある場合のみ処理
            if !names.is_empty() {
                // nameは&str
                // display_nameはOption<String>
                let name = names[0].display_name.as_ref().map(|s| s.as_str()).unwrap_or("default");
                let email_count = emails.len();
 
                // 各メールアドレスにニックネームを割り当て、CSVに書き込む
                for email in emails {
                    let email_address = email.value.unwrap_or_default();
                    let nickname = generate_nickname(&name, email_count, &mut existing_nicknames);
      			      writer.write_record(&[&nickname, name, &email_address])
      			          .map_err(|e| {
      			              eprintln!("アドレス帳への書き込みに失敗しました: {}", e);
      			              e
      			          })?;
                }
            }
        }
    };
 
    // CSVファイルへの書き込みを完了
    writer.flush().map_err(|e| {
		eprintln!("アドレス帳へ書き込みを完了できませんでした: {}", e);
		e
	})?;

    println!("アドレス帳がホームディレクトリにエクスポートされました。");
    Ok(())
}
