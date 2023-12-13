// 必要なクレートとモジュールをインポート
use yup_oauth2::{read_application_secret};
use hyper::client::Client;
use hyper_rustls::HttpsConnector;
use std::str::FromStr;
use google_people1::{PeopleService, oauth2, FieldMask};
use std::collections::HashSet;
use csv::WriterBuilder;

// ニックネームを生成する関数
fn generate_nickname(name: &str, email_count: usize, existing_nicknames: &mut HashSet<String>) -> String {
    // 名前の最後の部分（姓を想定）を取得し、それをベースとするニックネームを作成
	let last_name_part = name.split_whitespace().last().unwrap_or("Unknown").to_string();
	let base_nickname = last_name_part;

    // メールアドレスが複数ある場合は、カウンターを用いてユニークなニックネームを生成
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

// 非同期のメイン関数
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ホームディレクトリのパスを取得
    let home_dir = dirs::home_dir().expect("ホームディレクトリが見つかりません");
    // 認証情報とトークンキャッシュのファイルパスを設定
    let secret_file = home_dir.join(".client_secret.json");
    let token_cache_file = home_dir.join(".token_cache.json");

    // Google APIの認証情報を読み込み
    let secret = read_application_secret(secret_file).await?;

    // HTTPS対応のHTTPクライアントを構築
    let client = Client::builder().build(HttpsConnector::with_native_roots());

    // OAuth2認証フローを構築
    let auth = oauth2::InstalledFlowAuthenticator::builder(secret, oauth2::InstalledFlowReturnMethod::HTTPRedirect)
        .persist_tokens_to_disk(token_cache_file)
        .hyper_client(client)
        .build()
        .await?;

    // PeopleServiceを初期化
    let service = PeopleService::new(Client::builder().build(HttpsConnector::with_native_roots()), auth);

    // 連絡先情報を取得するためのフィールドマスクを設定
    let field_mask = FieldMask::from_str("names,emailAddresses");
    // Google People APIを使って連絡先情報を取得
    let results = service.people().connections_list("people/me")
        .page_size(1000)
        .person_fields(field_mask.unwrap())
        .doit().await?;

    // 既存のニックネームを保持するHashSetを初期化
    let mut existing_nicknames = HashSet::new();
    // アドレス帳ファイルのパスを設定
    let home_dir = dirs::home_dir().unwrap();
    let addressbook_path = home_dir.join(".addressbook");
    // CSVライターを初期化（タブ区切り）
    let mut writer = WriterBuilder::new()
            .delimiter(b'\t')  // タブ文字を区切り文字として設定
            .from_path(addressbook_path)?;

    // 取得した連絡先情報を処理
    if let Some(connections) = results.1.connections {
        for person in connections {
            // 名前とメールアドレスを取得
            let names = person.names.unwrap_or_else(Vec::new);
            let emails = person.email_addresses.unwrap_or_else(Vec::new);

            // 名前が存在する場合のみ処理
            if !names.is_empty() {
                let name = names[0].display_name.as_ref().map(|s| s.as_str()).unwrap_or("default");
                let email_count = emails.len();

                // 各メールアドレスに対してニックネームを生成し、CSVファイルに書き込む
                for email in emails {
                    let email_address = email.value.unwrap_or_default();
                    let nickname = generate_nickname(&name, email_count, &mut existing_nicknames);
                    writer.write_record(&[&nickname, name, &email_address])?;
                }
            }
        }
    }

    // CSVライターをフラッシュし、ファイルに書き込みを完了
    writer.flush()?;
    // 完了メッセージを表示
    println!("アドレス帳がホームディレクトリにエクスポートされました。");
    Ok(())
}

