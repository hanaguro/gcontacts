// 必要なクレートとモジュールをインポート
use yup_oauth2::{read_application_secret, authenticator::Authenticator}; // OAuth2認証用のモジュール
use hyper::client::{Client, HttpConnector}; // HTTPクライアント操作用
use hyper_rustls::HttpsConnector; // HTTPSサポート用
use std::str::FromStr; // 文字列からの型変換サポート用
use google_people1::{PeopleService, oauth2, FieldMask}; // Google People APIの利用
use std::collections::HashSet; // データの集合操作用
use csv::WriterBuilder; // CSVファイル書き込み用

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
    // 認証を行い、成功すれば処理を続行
    let auth = get_auth().await?;    

    // PeopleService（Google People APIクライアント）の初期化
    // このRustのコード行では、PeopleService::new関数を呼び出して、PeopleServiceオブジェクトを作成しています。
    // PeopleServiceは、Google People APIとのやり取りを行うためのクライアントです。このオブジェクトを使って、Google People APIを通じて連絡先の情報を取得したり、更新したりすることができます。
    // 
    // この関数呼び出しの構成要素を詳しく見てみましょう：
    //     Client::builder().build(HttpsConnector::with_native_roots()):
    //        Client::builder(): hyperクレートのClient型のビルダーを作成します。hyperはRustでHTTPクライアント機能を提供するクレートです。
    //       .build(HttpsConnector::with_native_roots()): Clientのビルダーを使用して新しいClientインスタンスを作成します。ここで、HttpsConnector::with_native_roots()を使用して、HTTPS通信を行うためのコネクタを指定しています。hyper_rustlsクレートのHttpsConnectorは、安全なTLS（以前のSSL）接続を提供するためのコンポーネントです。
    //      auth:
    //        これはAuthenticatorオブジェクトで、OAuth 2.0認証を行うために使用されます。これにより、Google People APIへの認証済みアクセスが可能になります。
    //      PeopleService::new(client, auth):
    //        ここで作成したClientインスタンス（HTTPSをサポートするHTTPクライアント）とAuthenticatorオブジェクトをPeopleServiceのコンストラクタに渡しています。これにより、Google People APIと通信するための設定が完了したPeopleServiceオブジェクトが作成されます。
    let service = PeopleService::new(Client::builder().build(HttpsConnector::with_native_roots()), auth);
 
    // Google People APIで取得するフィールドを設定
    let field_mask = FieldMask::from_str("names,emailAddresses");
 
    // Google People APIを使って連絡先情報を取得
    // PeopleService オブジェクトの connections_list メソッドを使って、指定されたGoogleアカウントの連絡先のリストを取得しています。ここで重要なのは以下の各ステップです：
    // service.people().connections_list("people/me"):
    // PeopleService のインスタンスである service を使用して、Google People APIの people エンドポイントにアクセスしています。
    // connections_list メソッドは、指定されたリソース（この場合は "people/me"、つまり認証されたユーザー自身）の連絡先のリストを取得するために使われます。
    // .page_size(1000):
    // このメソッドは、APIから一度に取得する連絡先の最大数を設定します。ここでは 1000 と指定されており、これは一回のAPIリクエストで最大1000件の連絡先を取得することを意味します。
    // .person_fields(field_mask.unwrap()):
    // person_fields メソッドは、取得したい連絡先の具体的なフィールドを指定します。この場合、FieldMask オブジェクト（field_mask）を使用しており、これには取得したいデータのフィールドが含まれています（例えば名前やメールアドレスなど）。
    // .doit().await?:
    // doit メソッドは、これまでに構築したAPIリクエストを実行し、結果を取得するために使われます。
    // await キーワードは、この非同期操作が完了するまで現在の関数の実行を一時停止し、完了後に結果を返します。
    // ? は、エラーが発生した場合に関数から早期にリターンするためのRustのエラーハンドリング構文です。
    // この部分のコードは、Google People APIから連絡先のデータを非同期に取得し、その結果を変数 results に格納しています。この結果には、ユーザーの連絡先リストが含まれており、その後のコードでこれらの情報に基づいて処理を行うことができます。
     let results = service.people().connections_list("people/me") 
       .page_size(1000)
       .person_fields(field_mask.unwrap())
       .doit().await?;

    // 生成されたニックネームを格納するHashSet
    let mut existing_nicknames = HashSet::new();
    // CSVファイルの保存場所を指定
    let home_dir = dirs::home_dir().unwrap();
 
    let addressbook_path = home_dir.join(".addressbook");
    // CSVファイルライターの初期化（タブ区切り）
    let mut writer = WriterBuilder::new()
            .delimiter(b'\t')
            .from_path(addressbook_path)?;
 
    // 取得した連絡先情報に基づいて処理
    if let Some(connections) = results.1.connections {
        for person in connections {
            // 各人物の名前とメールアドレスを取得
            let names = person.names.unwrap_or_else(Vec::new);
            let emails = person.email_addresses.unwrap_or_else(Vec::new);
 
            // 名前がある場合のみ処理
            if !names.is_empty() {
                let name = names[0].display_name.as_ref().map(|s| s.as_str()).unwrap_or("default");
                let email_count = emails.len();
 
                // 各メールアドレスにニックネームを割り当て、CSVに書き込む
                for email in emails {
                    let email_address = email.value.unwrap_or_default();
                    let nickname = generate_nickname(&name, email_count, &mut existing_nicknames);
                    writer.write_record(&[&nickname, name, &email_address])?;
                }
            }
        }
    };
 
    // CSVファイルへの書き込みを完了
    writer.flush()?;
    println!("アドレス帳がホームディレクトリにエクスポートされました。");

    Ok(())
}
