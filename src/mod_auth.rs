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

/// Google APIへのOAuth2認証を行う

use yup_oauth2::{InstalledFlowAuthenticator, InstalledFlowReturnMethod, read_application_secret, authenticator::Authenticator}; // OAuth2認証のためのモジュール
use hyper::client::{Client, HttpConnector}; // HTTPクライアント操作用
use hyper_rustls::HttpsConnector; // HTTPSサポート用

/// Google APIとの認証を行いAuthenticatorを返す非同期関数
pub async fn get_auth() -> Result<Authenticator<HttpsConnector<HttpConnector>>, Box<dyn std::error::Error>> {
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
