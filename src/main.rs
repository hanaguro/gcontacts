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
use base64::{engine::general_purpose, Engine as _};
use csv::WriterBuilder; // CSVファイルを書き込むため
use fluent::{bundle::FluentBundle, FluentResource}; // ローカライゼーション機能を提供するfluentクレート関連モジュール
use google_people1::{
    api::Biography, api::EmailAddress, api::Name, api::Nickname, api::Person, FieldMask,
    PeopleService,
}; // Google People APIを使用するため
use hyper::client::{Client, HttpConnector}; // HTTPクライアント操作用
use hyper_rustls::HttpsConnector; // HTTPSサポート用
use intl_memoizer::concurrent::IntlLangMemoizer; // 国際化機能を提供するintl_memoizerクレートのモジュール
use quoted_printable::decode as qp_decode; // Quoted-Printableエンコーディングをデコードするための関数 `decode` を `qp_decode` としてインポート。Quoted-Printableエンコードされた文字列のデコードに使用。
use std::collections::HashSet;
use std::env; // 環境変数を扱うための 'env' モジュールをインポート
use std::fs::File; // ファイル操作を行うための `File` クラスをインポート。ファイルの読み書きに使用。
use std::io::{self, BufRead}; // 入出力機能のための 'io' モジュールをインポート
use std::path::Path; // ファイルパスを扱うための 'Path' モジュールをインポート
use std::str; // 文字列のスライス操作を行うための `str` モジュールをインポート。文字列操作に使用。
use std::str::FromStr; // 文字列を型に変換するため // Base64エンコーディングのデコード操作を行うための `base64` クレートの一部をインポート。一般的なBase64デコード用途に使用。

mod mod_auth;
mod mod_fluent; // 'mod_fluent' モジュールをインポート。Fluent (国際化とローカリゼーション) ライブラリ関連の機能を提供します。
mod mod_locale; // 'mod_locale' モジュールをインポート。ロケールと言語設定に関連する機能を提供します。 // 'mod_auth' モジュールをインポート。認証プロセスに関連する機能を提供します。

// ユーザ選択
enum Select {
    Init, // 初期化オプション。例えば、初めてのデータ同期や設定の初期化に使用。
    Sync, // 同期オプション。データの同期や更新に使用。
}

enum UpdateSource {
    FromGoogle,      // 更新のソースとしてGoogleを選択。
    FromAddressBook, // 更新のソースとしてアドレス帳を選択。
}

// .addressbookの各行に格納されているデータ
#[derive(PartialEq, Eq)] // remove_related_apersons関数に必要。PartialEqトレイトを実装する。
#[derive(Clone)] // ここでCloneトレイトを導出する
struct APerson {
    nickname: String,  // ニックネーム。
    name: String,      // 実名または表示名。
    email: String,     // 電子メールアドレス。
    fcc: String,       // (未使用のプレースホルダーまたは特定の用途のためのフィールド)
    biography: String, // バイオグラフィーまたはユーザーに関する追加情報。
}

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
    println!(
        "\t{}",
        mod_fluent::get_translation(bundle, "app-description")
    );
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
        }
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
fn generate_nickname(
    name: &str,
    email_count: usize,
    existing_nicknames: &mut Vec<String>,
) -> String {
    // 名前の最後の部分を取得し、基本的なニックネームを作成
    let last_name_part = name
        .split_whitespace()
        .last()
        .unwrap_or("Unknown")
        .to_string();
    let base_nickname;
    let mut counter;

    // 既に存在するニックネームがある場合は、その最初のニックネームから数値部分を抽出
    if !existing_nicknames.is_empty() {
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

/// 文字列内でエンコードされた部分をデコードする。
///
/// この関数は、与えられた文字列をチェックし、Base64またはQuoted-Printableで
/// エンコードされている場合にデコードを行います。
///
/// # 引数
/// * `s` - デコードする必要があるかどうかをチェックする文字列への参照。
///
/// # 戻り値
/// `Result<String, String>` - 成功した場合はデコードされた文字列、
/// 失敗した場合はエラーメッセージを含むResultオブジェクト。
fn decode_if_encoded(s: &str) -> Result<String, String> {
    // 文字列の先頭の空白を取り除きます。これは、エンコードされた文字列が前に空白を含む可能性があるためです。
    let s = s.trim_start();

    // 文字列がBase64エンコード形式であるかチェックします。
    if s.starts_with("=?UTF-8?B?") && s.ends_with("?=") {
        // エンコードされた部分を抽出します。
        let encoded = &s[10..s.len() - 2];
        // Base64デコードを試みます。
        let decoded_bytes = general_purpose::STANDARD
            .decode(encoded)
            .map_err(|e| format!("Base64 decode error: {}", e))?;
        // デコードされたバイト列をUTF-8文字列に変換します。
        String::from_utf8(decoded_bytes).map_err(|e| format!("UTF-8 decode error: {}", e))
        // 文字列がQuoted-Printableエンコード形式であるかチェックします。
    } else if s.starts_with("=?UTF-8?Q?") && s.ends_with("?=") {
        // エンコードされた部分を抽出し、"_"を空白に置換します（Quoted-Printableの仕様に基づく）。
        let encoded = &s[10..s.len() - 2].replace("_", " ");
        // Quoted-Printableデコードを試みます。
        qp_decode(encoded.as_bytes(), quoted_printable::ParseMode::Robust)
            .map(|decoded_bytes| String::from_utf8_lossy(&decoded_bytes).into_owned())
            .map_err(|e| format!("Quoted-Printable decode error: {}", e))
    } else {
        // 文字列がエンコードされていない場合、そのまま返します。
        Ok(s.to_string())
    }
}

/// 与えられたフィールドから `APerson` 構造体を生成し、ベクターに追加する。
///
/// この関数は、文字列のベクター（`fields`）を取り、各フィールドをデコードして
/// `APerson` 構造体を生成します。生成された `APerson` は引数として渡された
/// `APerson` 構造体のベクター（`persons`）に追加されます。フィールドのデコードに失敗した場合は、
/// エラーが返されます。
///
/// # 引数
/// * `persons` - `APerson` 構造体を追加するためのベクターへの可変参照。
/// * `fields` - デコードする必要があるフィールドのベクター。通常はタブ区切りの文字列から分割されたもの。
///
/// # 戻り値
/// `Result<(), Box<dyn std::error::Error>>` - 処理が成功した場合は `Ok(())` を、失敗した場合はエラーを含む `Result` を返します。
fn get_decoded_apersons(
    persons: &mut Vec<APerson>,
    fields: Vec<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    // 各フィールドをデコードし、`APerson` 構造体に変換します。
    let nickname = decode_if_encoded(fields.get(0).unwrap_or(&""))?;
    let name = decode_if_encoded(fields.get(1).unwrap_or(&""))?;
    let email = decode_if_encoded(fields.get(2).unwrap_or(&""))?;
    let fcc = decode_if_encoded(fields.get(3).unwrap_or(&""))?;
    let biography = decode_if_encoded(fields.get(4).unwrap_or(&""))?;

    // `APerson` 構造体をベクトルに追加します。
    persons.push(APerson {
        nickname,
        name,
        email,
        fcc,
        biography,
    });

    Ok(())
}

/// 与えられた行を解析し、APerson構造体に変換してVecに追加する関数。
///
/// この関数は、タブ区切りの文字列（`combined_line`）を取得し、それをフィールドに分割して、
/// それらのフィールドから`APerson`構造体を作成し、与えられた`APerson`のVec（`persons`）に追加します。
/// フィールドの数が5つを超える場合はエラーを返します。
///
/// # 引数
/// * `persons` - `APerson`構造体を追加するためのVecへの可変参照。
/// * `combined_line` - 解析するための行への可変参照。
///
/// # 戻り値
/// `Result<(), Box<dyn std::error::Error>>` - 成功した場合はOk(())、失敗した場合はエラー。
fn convert_line_to_aperson(
    mut persons: &mut Vec<APerson>,
    combined_line: &mut String,
) -> Result<(), Box<dyn std::error::Error>> {
    // タブで区切られたフィールドに分割
    let fields: Vec<&str> = combined_line.split('\t').collect();

    // フィールドの数が多すぎる場合はエラーを返す
    if fields.len() > 5 {
        // エラーメッセージを Box<dyn Error> に変換して返す
        return Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Record has too many fields",
        )));
    }

    // 各フィールドをデコードし、`APerson` 構造体に変換
    get_decoded_apersons(&mut persons, fields)?;
    // 結合された行をクリアして、次の行の処理に備える
    combined_line.clear();

    Ok(())
}

/// '.addressbook' ファイルからデータを読み込み、APerson構造体のベクターを返す。
///
/// この関数は、指定されたパスの'.addressbook' ファイルを開き、その内容を読み込み、
/// 各行をAPerson構造体に変換してベクターに格納します。
///
/// # 引数
/// * `file_path` - '.addressbook' ファイルのパスへの参照。
///
/// # 戻り値
/// `Result<Vec<APerson>, String>` - 成功した場合はAPersonオブジェクトのベクター、
/// 失敗した場合はエラーメッセージを含むResultオブジェクト。
fn load_addressbook_data(file_path: &Path) -> Result<Vec<APerson>, Box<dyn std::error::Error>> {
    // `APerson` 構造体のベクトルを初期化します。
    let mut persons: Vec<APerson> = Vec::new();

    // 指定されたファイルを開きます。エラーが発生した場合はエラーメッセージを返します。
    let file = File::open(file_path).map_err(|e| e.to_string())?;

    // 結合された行を格納するための文字列を初期化します。
    let mut combined_line = String::new();

    // ファイルの各行を読み込みます。
    for line in io::BufReader::new(file).lines() {
        let line = line.map_err(|e| e.to_string())?;

        // 行がタブ文字で終わっている場合は、次の行と結合する必要があります。
        // biographyが空の場合、タブ文字で終わっている可能性があるので考慮する必要あり
        if line.ends_with('\t') && (combined_line.chars().filter(|&c| c == '\t').count() < 4) {
            // 1つのaperson構造体に含まれる最大のタブ文字は4なので4の場合は除外
            combined_line.push_str(&line);
        } else {
            if !line.starts_with("   ") {
                convert_line_to_aperson(&mut persons, &mut combined_line)?;
            }

            combined_line.push_str(&line);
            convert_line_to_aperson(&mut persons, &mut combined_line)?;
        }
    }

    // ファイルの最後の行がタブ文字で終わっている場合
    if !combined_line.is_empty() {
        convert_line_to_aperson(&mut persons, &mut combined_line)?;
    }

    // 処理が完了したら、`APerson` 構造体のベクトルを返します。
    Ok(persons)
}

/// Googleの連絡先を更新する非同期関数。
///
/// 既存のGoogleの連絡先（Personオブジェクト）を更新するか、新しい連絡先を作成します。
/// 更新するには、既存のPersonオブジェクトの参照とAPersonオブジェクトが必要です。
///
/// # 引数
/// * `gperson_option` - 既存のGoogleの連絡先のOption参照。Noneの場合は新しい連絡先を作成。
/// * `aperson` - 更新するためのAPersonオブジェクトの参照。
/// * `service` - PeopleServiceの参照。Google People APIへのリクエストに使用。
///
/// # 戻り値
/// `Result<(), Box<dyn std::error::Error>>` - 成功した場合はOk(())、失敗した場合はエラー。
async fn update_google_contacts(
    gperson_option: Option<&Person>,
    aperson: &APerson,
    service: &PeopleService<HttpsConnector<HttpConnector>>,
) -> Result<(), Box<dyn std::error::Error>> {
    // 新しいPersonインスタンスを作成するか、既存の参照を使用して更新
    let new_gperson = match gperson_option {
        Some(person) => {
            // 既存のデータをコピーし、必要なフィールドのみを更新
            let mut updated_person = person.clone();

            let existing_metadata = person
                .nicknames
                .as_ref()
                .and_then(|n| n.get(0).and_then(|nn| nn.metadata.clone()));
            let existing_type = person
                .nicknames
                .as_ref()
                .and_then(|n| n.get(0).and_then(|nn| nn.type_.clone()));

            updated_person.nicknames = Some(vec![Nickname {
                value: Some(aperson.nickname.clone()),
                metadata: existing_metadata,
                type_: existing_type,
            }]);

            // 各フィールドの既存のメタデータを取得し、新しいフィールドの値を設定
            // 以下、ニックネーム、名前、メールアドレス、バイオグラフィの更新処理
            let existing_display_name_last_first = person
                .names
                .as_ref()
                .and_then(|n| n.get(0).and_then(|nn| nn.display_name_last_first.clone()));
            let existing_family_name = person
                .names
                .as_ref()
                .and_then(|n| n.get(0).and_then(|nn| nn.family_name.clone()));
            let existing_given_name = person
                .names
                .as_ref()
                .and_then(|n| n.get(0).and_then(|nn| nn.given_name.clone()));
            let existing_honorific_prefix = person
                .names
                .as_ref()
                .and_then(|n| n.get(0).and_then(|nn| nn.honorific_prefix.clone()));
            let existing_honorific_suffix = person
                .names
                .as_ref()
                .and_then(|n| n.get(0).and_then(|nn| nn.honorific_suffix.clone()));
            let existing_metadata = person
                .names
                .as_ref()
                .and_then(|n| n.get(0).and_then(|nn| nn.metadata.clone()));
            let existing_middle_name = person
                .names
                .as_ref()
                .and_then(|n| n.get(0).and_then(|nn| nn.middle_name.clone()));
            let existing_phonetic_family_name = person
                .names
                .as_ref()
                .and_then(|n| n.get(0).and_then(|nn| nn.phonetic_family_name.clone()));
            let existing_phonetic_full_name = person
                .names
                .as_ref()
                .and_then(|n| n.get(0).and_then(|nn| nn.phonetic_full_name.clone()));
            let existing_phonetic_given_name = person
                .names
                .as_ref()
                .and_then(|n| n.get(0).and_then(|nn| nn.phonetic_given_name.clone()));
            let existing_phonetic_honorific_prefix = person
                .names
                .as_ref()
                .and_then(|n| n.get(0).and_then(|nn| nn.phonetic_honorific_prefix.clone()));
            let existing_phonetic_honorific_suffix = person
                .names
                .as_ref()
                .and_then(|n| n.get(0).and_then(|nn| nn.phonetic_honorific_suffix.clone()));
            let existing_phonetic_middle_name = person
                .names
                .as_ref()
                .and_then(|n| n.get(0).and_then(|nn| nn.phonetic_middle_name.clone()));
            let existing_unstructured_name = person
                .names
                .as_ref()
                .and_then(|n| n.get(0).and_then(|nn| nn.unstructured_name.clone()));
            updated_person.names = Some(vec![Name {
                display_name: Some(aperson.name.clone()),
                display_name_last_first: existing_display_name_last_first,
                family_name: existing_family_name,
                given_name: existing_given_name,
                honorific_prefix: existing_honorific_prefix,
                honorific_suffix: existing_honorific_suffix,
                metadata: existing_metadata,
                middle_name: existing_middle_name,
                phonetic_family_name: existing_phonetic_family_name,
                phonetic_full_name: existing_phonetic_full_name,
                phonetic_given_name: existing_phonetic_given_name,
                phonetic_honorific_prefix: existing_phonetic_honorific_prefix,
                phonetic_honorific_suffix: existing_phonetic_honorific_suffix,
                phonetic_middle_name: existing_phonetic_middle_name,
                unstructured_name: existing_unstructured_name,
            }]);

            let existing_metadata = person
                .email_addresses
                .as_ref()
                .and_then(|n| n.get(0).and_then(|nn| nn.metadata.clone()));
            let existing_type_ = person
                .email_addresses
                .as_ref()
                .and_then(|n| n.get(0).and_then(|nn| nn.type_.clone()));
            let existing_formatted_type = person
                .email_addresses
                .as_ref()
                .and_then(|n| n.get(0).and_then(|nn| nn.formatted_type.clone()));
            let existing_display_name = person
                .email_addresses
                .as_ref()
                .and_then(|n| n.get(0).and_then(|nn| nn.display_name.clone()));
            updated_person.email_addresses = Some(vec![EmailAddress {
                value: Some(aperson.email.clone()),
                metadata: existing_metadata,
                type_: existing_type_,
                formatted_type: existing_formatted_type,
                display_name: existing_display_name,
            }]);

            let existing_metadata = person
                .biographies
                .as_ref()
                .and_then(|n| n.get(0).and_then(|nn| nn.metadata.clone()));
            let existing_content_type = person
                .biographies
                .as_ref()
                .and_then(|n| n.get(0).and_then(|nn| nn.content_type.clone()));
            updated_person.biographies = Some(vec![Biography {
                value: Some(aperson.biography.clone()),
                metadata: existing_metadata,
                content_type: existing_content_type,
            }]);
            updated_person
        }
        None => {
            // 新しいPersonオブジェクトを作成
            let mut new_person = Person::default();

            // 各フィールドの値をAPersonからコピー
            new_person.nicknames = Some(vec![Nickname {
                value: Some(aperson.nickname.clone()),
                metadata: None,
                type_: None,
            }]);

            let first_name;
            let last_name;
            let words: Vec<&str> = aperson.name.split_whitespace().collect();
            if words.len() >= 2 {
                first_name = words[0];
                last_name = match words.last() {
                    Some(s) => s,
                    None => "",
                }
            } else {
                first_name = aperson.name.as_str();
                last_name = "";
            }

            new_person.names = Some(vec![Name {
                display_name: Some(aperson.name.clone()),
                display_name_last_first: None,
                family_name: Some(last_name.to_string()),
                given_name: Some(first_name.to_string()),
                honorific_prefix: None,
                honorific_suffix: None,
                metadata: None,
                middle_name: None,
                phonetic_family_name: None,
                phonetic_full_name: None,
                phonetic_given_name: None,
                phonetic_honorific_prefix: None,
                phonetic_honorific_suffix: None,
                phonetic_middle_name: None,
                unstructured_name: None,
            }]);
            new_person.email_addresses = Some(vec![EmailAddress {
                value: Some(aperson.email.clone()),
                metadata: None,
                type_: None,
                formatted_type: None,
                display_name: None,
            }]);
            new_person.biographies = Some(vec![Biography {
                value: Some(aperson.biography.clone()),
                metadata: None,
                content_type: None,
            }]);
            new_person
        }
    };

    // 更新するフィールドのマスクを設定
    let field_mask = FieldMask::from_str("nicknames,names,emailAddresses,biographies").unwrap();

    // Personオブジェクトのresource_nameがあれば、Google People APIを使用して更新
    if let Some(resource_name) = new_gperson.resource_name.as_ref() {
        service
            .people()
            .update_contact(new_gperson.clone(), resource_name)
            .update_person_fields(field_mask)
            .doit()
            .await?;
    } else {
        // resource_nameがない場合は、新規登録を行う
        service
            .people()
            .create_contact(new_gperson.clone())
            .person_fields(field_mask)
            .doit()
            .await?;
    }
    Ok(())
}

/// 特定のGoogleのPersonオブジェクトを削除する非同期関数。
///
/// この関数は、Google People APIを使用してGoogleの連絡先リストから特定のPersonオブジェクトを削除します。
/// 削除対象のPersonオブジェクトは、その`resource_name`プロパティに基づいて識別されます。
/// `resource_name`が存在しない場合、関数はエラーを返します。
///
/// # 引数
/// * `gperson` - 削除するPersonオブジェクトへの参照。
/// * `service` - Google People APIにアクセスするためのPeopleServiceオブジェクトへの参照。
///
/// # 戻り値
/// `Result<(), Box<dyn std::error::Error>>` - 処理が成功した場合はOk(())、
/// 失敗した場合はエラーを含むResultオブジェクト。
async fn remove_related_gperson(
    gperson: &Person,
    service: &PeopleService<HttpsConnector<HttpConnector>>,
) -> Result<(), Box<dyn std::error::Error>> {
    // gpersonからresource_nameを取得し、その存在をチェック
    if let Some(resource_name) = gperson.resource_name.as_ref() {
        // resource_nameが存在する場合、PeopleServiceを使用して削除リクエストを行う
        service
            .people()
            .delete_contact(resource_name)
            .doit()
            .await?;
    } else {
        // resource_nameが存在しない場合、エラーを返す
        return Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "resource name is empty.",
        )));
    }

    // 処理が正常に完了した場合、Ok(())を返す
    Ok(())
}

/// 特定のメールアドレスを持つGoogleのPersonオブジェクトを取得する。
///
/// 与えられたメールアドレスと一致するメールアドレスを持つPersonオブジェクトを`gpersons`ベクターから探し出し、
/// それらに関連するPersonオブジェクトの参照のベクターを返します。
///
/// # 引数
/// * `gpersons` - GoogleのPersonオブジェクトのベクターへの参照。
/// * `email` - 検索するメールアドレスの参照。
///
/// # 戻り値
/// `Vec<&'a Person>` - 与えられたメールアドレスを持つPersonオブジェクトの参照のベクター。
fn get_related_gpersons<'a>(gpersons: &'a Vec<Person>, email: &String) -> Vec<&'a Person> {
    // gpersonsベクターをイテレートし、条件に合致するPersonオブジェクトの参照をフィルタリング
    let related_persons = gpersons
        .iter()
        .filter(
            |&p|
        // Personオブジェクトのemail_addressesフィールドをチェック
        p.email_addresses.as_ref().unwrap_or(&Vec::new())
        // email_addresses内の各EmailAddressオブジェクトに対して、
        // 与えられたemailと一致するかどうかを確認
        .iter()
        .any(|e| e.value.as_ref() == Some(email)), // 条件に合致するPersonオブジェクトの参照をベクターとして収集
        )
        .collect::<Vec<&Person>>();

    // 関連するPersonオブジェクトの参照のベクターを返す
    related_persons
}

/// 特定のメールアドレスに関連する `APerson` オブジェクトの参照を取得する。
///
/// 与えられたメールアドレスと一致する `APerson` オブジェクトを `people` ベクターから探し出し、
/// それらに関連する `APerson` オブジェクトの参照のベクターを返します。
///
/// # 引数
/// * `people` - `APerson` オブジェクトのベクターへの参照。
/// * `email_to_find` - 検索するメールアドレス。
///
/// # 戻り値
/// `Vec<&'a APerson>` - 与えられたメールアドレスを持つ `APerson` オブジェクトの参照のベクター。
fn get_related_apersons<'a>(people: &'a Vec<APerson>, email_to_find: &str) -> Vec<&'a APerson> {
    // `people` ベクターをイテレートし、条件に合致する `APerson` オブジェクトの参照をフィルタリング
    people
        .iter()
        // `APerson` オブジェクトの email フィールドが `email_to_find` と一致するか確認
        .filter(|person| person.email == email_to_find)
        // 条件に合致する `APerson` オブジェクトの参照をベクターとして収集
        .collect()
}

/// 特定の `APerson` オブジェクトを `apeople` ベクターから削除する。
///
/// 与えられた `related_apeople` に含まれる `APerson` オブジェクトの参照に一致する
/// オブジェクトを `apeople` ベクターから削除します。
///
/// # 引数
/// * `apeople` - `APerson` オブジェクトのベクターへの可変参照。
/// * `related_apeople` - 削除する `APerson` オブジェクトの参照のベクター。
fn remove_related_apersons<'a>(apeople: &mut Vec<APerson>, related_apeople: &Vec<&'a APerson>) {
    // `apeople` ベクターから `related_apeople` に含まれるオブジェクトを削除
    apeople.retain(|ap|
        // `related_apeople` に含まれていない `APerson` オブジェクトだけを保持
        !related_apeople.contains(&ap));
}

/// 特定の `APerson` オブジェクトに関連するGoogleのPersonオブジェクトを検索する関数。
///
/// この関数は、指定された `APerson` オブジェクトのデータ（ニックネーム、名前、メールアドレス、バイオグラフィ）に基づいて、
/// Google People APIを通じて取得されたPersonオブジェクトのリスト（`gpersons`）の中から関連するオブジェクトを検索します。
/// 条件に一致するGoogleのPersonオブジェクトが見つかった場合、それらを含むベクターが返されます。
///
/// # 引数
/// * `gpersons` - GoogleのPersonオブジェクトのベクターへの参照。
/// * `aperson` - 検索基準となる `APerson` オブジェクトへの参照。
///
/// # 戻り値
/// `Option<Vec<Person>>` - 条件に一致するPersonオブジェクトのベクター。
/// 一致するオブジェクトがない場合は `None` を返す。
fn get_gpersons_from_aperson(gpersons: &Vec<Person>, aperson: &APerson) -> Option<Vec<Person>> {
    // 条件に一致するPersonオブジェクトをフィルタリングして収集
    let filtered_gpersons: Vec<Person> = gpersons
        .iter()
        .filter(|gperson| {
            // Email と Nickname の条件をチェック
            let email_match = gperson.email_addresses.as_ref().map_or(false, |emails| {
                emails
                    .iter()
                    .any(|email| email.value.as_ref() == Some(&aperson.email))
            });
            let nickname_match = gperson.nicknames.as_ref().map_or(false, |nicknames| {
                nicknames
                    .iter()
                    .any(|nickname| nickname.value.as_ref() == Some(&aperson.nickname))
            });

            // Name または Organization の条件をチェック
            let name_match = gperson.names.as_ref().map_or(false, |names| {
                names
                    .iter()
                    .any(|name| name.display_name.as_ref() == Some(&aperson.name))
            }) || gperson.organizations.as_ref().map_or(false, |orgs| {
                orgs.iter()
                    .any(|org| org.name.as_ref() == Some(&aperson.name))
            });

            let biography_match = gperson.biographies.as_ref().map_or(false, |biographies| {
                biographies
                    .iter()
                    .any(|bio| bio.value.as_ref() == Some(&aperson.biography))
            });

            // すべての条件が true であれば true を返す
            email_match && nickname_match && name_match && biography_match
        })
        .cloned()
        .collect();

    // フィルタリングされた結果が空でなければ、その結果を返す
    if filtered_gpersons.is_empty() {
        None
    } else {
        Some(filtered_gpersons)
    }
}

/// GoogleのPersonオブジェクトから名前を取得する関数。
///
/// この関数は、指定されたGoogleのPersonオブジェクトから名前を抽出します。Personオブジェクトに名前が存在する場合、
/// 最初に見つかった名前を返します。名前が存在しない場合は、代わりに所属組織名を返します。どちらも存在しない場合は空文字列を返します。
///
/// # 引数
/// * `person` - 名前を取得するGoogleのPersonオブジェクトへの参照。
///
/// # 戻り値
/// `String` - 取得した名前。名前または所属組織名が存在しない場合は空文字列。
fn get_gcontact_name(person: &Person) -> String {
    // 名前を格納する変数を初期化
    let mut gname;

    // Personオブジェクトのnamesフィールドを確認し、名前が存在するかチェック
    match &person.names {
        Some(names) => {
            // 名前が存在する場合
            if names.is_empty() {
                // 名前のリストが空の場合、空文字列を代入
                gname = "".to_string();
            } else {
                // 名前のリストが空でない場合、最初の名前を使用
                gname = names[0].display_name.clone().unwrap_or_default();
            }
        }
        None => {
            // 名前が存在しない場合、空文字列を代入
            gname = "".to_string();
        }
    };

    // 名前が空の場合、Personオブジェクトのorganizationsフィールドを確認
    if gname.is_empty() {
        match &person.organizations {
            Some(organizations) => {
                // 所属組織が存在する場合
                if organizations.is_empty() {
                    // 所属組織のリストが空の場合、空文字列を代入
                    gname = "".to_string();
                } else {
                    // 所属組織のリストが空でない場合、最初の所属組織名を使用
                    gname = organizations[0].name.clone().unwrap_or_default();
                }
            }
            None => {
                // 所属組織が存在しない場合、空文字列を代入
                gname = "".to_string();
            }
        };
    }

    // 取得した名前または所属組織名を返す
    gname
}

/// GoogleのPersonオブジェクトからニックネームを取得する関数。
///
/// この関数は、指定されたGoogleのPersonオブジェクトからニックネームを抽出します。
/// Personオブジェクトにニックネームが存在する場合、最初に見つかったニックネームを返します。
/// ニックネームが存在しない場合は空文字列を返します。
///
/// # 引数
/// * `person` - ニックネームを取得するGoogleのPersonオブジェクトへの参照。
///
/// # 戻り値
/// `String` - 取得したニックネーム。ニックネームが存在しない場合は空文字列。
fn get_gcontact_nickname(person: &Person) -> String {
    // ニックネームを格納する変数を初期化
    let gnickname;

    // Personオブジェクトのnicknamesフィールドを確認し、ニックネームが存在するかチェック
    match &person.nicknames {
        Some(nicknames) => {
            // ニックネームが存在する場合
            if nicknames.is_empty() {
                // ニックネームのリストが空の場合、空文字列を代入
                gnickname = "".to_string();
            } else {
                // ニックネームのリストが空でない場合、最初のニックネームを使用
                gnickname = nicknames[0].value.clone().unwrap_or_default();
            }
        }
        None => {
            // ニックネームが存在しない場合、空文字列を代入
            gnickname = "".to_string();
        }
    };

    // 取得したニックネームを返す
    gnickname
}

/// GoogleのPersonオブジェクトからバイオグラフィーを取得する関数。
///
/// この関数は、指定されたGoogleのPersonオブジェクトからバイオグラフィー（自己紹介やメモなどの情報）を抽出します。
/// Personオブジェクトにバイオグラフィーが存在する場合、最初に見つかったバイオグラフィーを返します。
/// バイオグラフィーが存在しない場合は空文字列を返します。
///
/// # 引数
/// * `person` - バイオグラフィーを取得するGoogleのPersonオブジェクトへの参照。
///
/// # 戻り値
/// `String` - 取得したバイオグラフィー。バイオグラフィーが存在しない場合は空文字列。
fn get_gcontact_biography(person: &Person) -> String {
    // バイオグラフィーを格納する変数を初期化
    let gbiography;

    // Personオブジェクトのbiographiesフィールドを確認し、バイオグラフィーが存在するかチェック
    match &person.biographies {
        Some(biographies) => {
            // バイオグラフィーが存在する場合
            if biographies.is_empty() {
                // バイオグラフィーのリストが空の場合、空文字列を代入
                gbiography = "".to_string();
            } else {
                // バイオグラフィーのリストが空でない場合、最初のバイオグラフィーを使用
                gbiography = biographies[0].value.clone().unwrap_or_default();
            }
        }
        None => {
            // バイオグラフィーが存在しない場合、空文字列を代入
            gbiography = "".to_string();
        }
    };

    // 取得したバイオグラフィーを返す
    gbiography
}

/// ユーザー入力に基づいてデータ更新のソースを選択する関数。
///
/// この関数は、ユーザーにGoogle Contactsと.addressbookのどちらをデータ更新のソースとして使用するかを尋ね、
/// 入力に基づいて適切な `UpdateSource` 列挙型を返します。ユーザーが 'g' を入力した場合は `UpdateSource::FromGoogle` を、
/// 'a' を入力した場合は `UpdateSource::FromAddressBook` を返します。入力が 'g' または 'a' 以外の場合は、
/// オペレーションをキャンセルします。
///
/// # 引数
/// * `bundle` - ローカライズされた文字列と国際化の詳細を含むFluentBundleへの参照。
///
/// # 戻り値
/// `UpdateSource` - ユーザーが選択したデータ更新のソース。
fn input_select_source(bundle: &FluentBundle<FluentResource, IntlLangMemoizer>) -> UpdateSource {
    // デフォルトのデータソースをGoogleに設定
    let mut source = UpdateSource::FromGoogle;

    // ユーザー入力を取得するためのバッファ
    let mut input = String::new();
    // 標準入力からの読み取りを試み、エラーがあれば処理を終了
    if let Err(e) = io::stdin().read_line(&mut input) {
        eprintln!(
            "{}: {}",
            mod_fluent::get_translation(&bundle, "input-error"),
            e
        );
        std::process::exit(0);
    }

    // ユーザー入力により、Google Contactsまたは.addressbookのどちらのデータを優先するか決定
    if (input.trim().to_lowercase() != "g") && (input.trim().to_lowercase() != "a") {
        println!("{}", mod_fluent::get_translation(&bundle, "op-cancel"));
        std::process::exit(0);
    } else if input.trim().to_lowercase() == "g" {
        // Google Contactsを優先し、.addressbookを更新する
        source = UpdateSource::FromGoogle;
    } else if input.trim().to_lowercase() == "a" {
        // .addressbookを優先し、Google Contactsを更新する
        source = UpdateSource::FromAddressBook;
    }

    // 選択されたデータソースを返す
    source
}

// 非同期のメイン関数
#[tokio::main]
async fn main() {
    // ロケールの設定（コマンドライン引数、環境変数、既定値などから）
    // LANG環境変数からロケールを取得する
    let locale = mod_locale::get_locale_from_env();
    // Fluentバンドルを初期化
    let bundle = mod_fluent::init_fluent_bundle(&locale);

    // コマンドライン引数を取得
    let args: Vec<String> = env::args().collect();

    let sel;

    // --help オプションのチェック
    if args.contains(&"--help".to_string()) {
        // ヘルプメッセージを表示
        print_help(&bundle);
        std::process::exit(0);
    } else if args.contains(&"init".to_string()) {
        // Google Contactsからダウンロードして.addressbookを上書き
        sel = Select::Init;
    } else if args.contains(&"sync".to_string()) {
        // Google Concatcsのデータと同期
        sel = Select::Sync;
    } else {
        eprintln!("{}", mod_fluent::get_translation(&bundle, "no-option"));
        std::process::exit(1);
    }

    // 認証が成功した場合の処理を続行
    let auth = match mod_auth::get_auth().await {
        Ok(a) => a,
        Err(e) => {
            eprintln!(
                "{}: {}",
                mod_fluent::get_translation(&bundle, "auth-error"),
                e
            );
            std::process::exit(1);
        }
    };

    // PeopleService（Google People APIクライアント）を初期化
    let service = PeopleService::new(
        Client::builder().build(HttpsConnector::with_native_roots()),
        auth,
    );

    // Google People APIから取得するフィールドを設定
    let field_mask =
        FieldMask::from_str("nicknames,names,organizations,emailAddresses,biographies").unwrap(); // 失敗したらパニック

    // Google People APIを使用して連絡先情報を取得
    // resultsは(Response<Body>, ListConnectionsResponse)のタプル
    let results = service
        .people()
        .connections_list("people/me")
        .page_size(1000)
        .person_fields(field_mask.clone())
        .doit()
        .await
        .unwrap_or_else(|e| {
            eprintln!(
                "{}: {}",
                mod_fluent::get_translation(&bundle, "fail-contact"),
                e
            );
            std::process::exit(1);
        });

    // CSVファイルの保存場所を指定
    let home_dir = dirs::home_dir().unwrap_or_else(|| {
        eprintln!("{}", mod_fluent::get_translation(&bundle, "home-notfound"));
        std::process::exit(1);
    });

    let addressbook_path = home_dir.join(".addressbook");

    // ユーザの選択に応じた処理を行なう
    match sel {
        Select::Init => {
            // Google Contactsからダウンロードし、.addressbookに上書きする
            // ファイルが存在するかチェックし、存在する場合は上書き確認する
            if Path::new(&addressbook_path).exists() {
                println!(
                    "{}",
                    mod_fluent::get_translation(&bundle, "overwrite-or-not")
                );
                let mut input = String::new();
                if let Err(e) = io::stdin().read_line(&mut input) {
                    eprintln!(
                        "{}: {}",
                        mod_fluent::get_translation(&bundle, "input-error"),
                        e
                    );
                    std::process::exit(1);
                };

                // y以外を選択していたらキャンセル
                if input.trim().to_lowercase() != "y" {
                    println!("{}", mod_fluent::get_translation(&bundle, "op-cancel"));
                    std::process::exit(1);
                }
            }

            // CSVファイルライター（タブ区切り）を初期化
            let mut writer = WriterBuilder::new()
                .delimiter(b'\t')
                .from_path(addressbook_path)
                .unwrap_or_else(|e| {
                    eprintln!(
                        "{}: {}",
                        mod_fluent::get_translation(&bundle, "init-error"),
                        e
                    );
                    std::process::exit(1);
                });

            // 取得した連絡先情報に基づいて処理
            if let Some(connections) = results.1.connections {
                for person in connections {
                    // 生成されたニックネームを格納するVec
                    let mut existing_nicknames = Vec::new();

                    // Google Contactsから各人物の名前と会社を取得する
                    let person_clone = person.clone();
                    let names = person_clone.names.unwrap_or_else(Vec::new);
                    let organizations = person_clone.organizations.unwrap_or_else(Vec::new);
                    let emails = person_clone.email_addresses.unwrap_or_else(Vec::new);

                    // 名前が存在する場合のみ処理
                    if !names.is_empty() || !organizations.is_empty() {
                        // ニックネームを取得する
                        let nickname_from_g = get_gcontact_nickname(&person);
                        if !nickname_from_g.is_empty() {
                            existing_nicknames.push(nickname_from_g);
                        }

                        // 名前か会社を取得する
                        let name = get_gcontact_name(&person);

                        // メモ欄の内容を取得する
                        let memo = get_gcontact_biography(&person);

                        let email_count = emails.len();

                        // 各メールアドレスにニックネームを割り当ててCSVに書き込む
                        for email in emails {
                            let email_address = email.value.unwrap_or_default();
                            let nickname =
                                generate_nickname(&name, email_count, &mut existing_nicknames);
                            if let Err(e) =
                                writer.write_record(&[&nickname, &name, &email_address, "", &memo])
                            {
                                eprintln!(
                                    "{}: {}",
                                    mod_fluent::get_translation(&bundle, "write-error"),
                                    e
                                );
                                std::process::exit(1);
                            }
                        }
                    }
                }
            };

            // CSVファイルへの書き込みを完了
            if let Err(e) = writer.flush() {
                eprintln!(
                    "{}: {}",
                    mod_fluent::get_translation(&bundle, "flush-error"),
                    e
                );
                std::process::exit(1);
            };

            // 書き込み完了メッセージを表示
            println!(
                "{}",
                mod_fluent::get_translation(&bundle, "export-complete")
            );
        }

        Select::Sync => {
            // Google Contactsと.adressbookを同期する

            // .addressbook書き込みフラグ
            let mut apeople_diarty = false;

            // .addressbookからデータを全て取得
            let mut apeople =
                load_addressbook_data(addressbook_path.as_path()).unwrap_or_else(|e| {
                    eprintln!(
                        "{}: {}",
                        mod_fluent::get_translation(&bundle, "fail-addressbook"),
                        e
                    );
                    std::process::exit(1);
                });

            // Google Contactsからデータを全て取得
            let gpersons = results.1.connections.unwrap_or_else(|| {
                eprintln!(
                    "{}",
                    mod_fluent::get_translation(&bundle, "fail-google-contacts")
                );
                std::process::exit(1);
            });

            // .addressbookのメールアドレスと比較するためのHashSet
            let mut gperson_emails: HashSet<String> = HashSet::new();
            for gperson in gpersons.clone() {
                // gpersonのメールアドレスのvecを取得
                let emails = match gperson.email_addresses {
                    Some(s) => s,
                    None => continue,
                };

                // メールアドレスのvecからメールアドレスを取得
                for email in emails {
                    // メールアドレスをStringに変換
                    let email_str = match email.value {
                        Some(s) => s,
                        None => continue,
                    };
                    gperson_emails.insert(email_str);
                }
            }

            // Google Contactsのメールアドレスと比較するためのHashSet
            let aperson_emails: HashSet<String> =
                apeople.iter().map(|ap| ap.email.clone()).collect();

            // 一方にのみ存在するメールアドレスを特定
            let unique_to_gpersons = gperson_emails.difference(&aperson_emails);
            let unique_to_apeople = aperson_emails.difference(&gperson_emails);
            // 両方に存在するメールアドレスを特定
            let common_emails = aperson_emails.intersection(&gperson_emails);

            for email in unique_to_gpersons {
                // このメールアドレスはGoogle Contactsにのみ存在し、.addressbookには存在しない。
                if email.is_empty() {
                    continue;
                }

                // Google Contactsの中でこのメールアドレスを持つ人々
                let related_gpersons = get_related_gpersons(&gpersons, email);

                let mut source;
                for gperson in &related_gpersons {
                    // .addressbookに新規登録するか、Google Contactsから削除するかを入力させる
                    println!(
                        "{}",
                        mod_fluent::get_translation(&bundle, "add-a-or-delete-g-mode")
                    );
                    let gnickname = get_gcontact_nickname(gperson);
                    let gname = get_gcontact_name(gperson);
                    let gbiography = get_gcontact_biography(gperson);
                    println!(
                        "Google Contacts   :{}/{}/{}/{}",
                        gnickname, gname, email, gbiography
                    );

                    source = input_select_source(&bundle);

                    // ユーザ入力に従って分岐
                    match source {
                        UpdateSource::FromGoogle => {
                            // Google Contactsから削除する
                            match remove_related_gperson(&gperson, &service).await {
                                Ok(()) => {
                                    println!(
                                        "{}",
                                        mod_fluent::get_translation(
                                            &bundle,
                                            "update-success-google-contacts"
                                        )
                                    );
                                }
                                Err(e) => {
                                    eprintln!(
                                        "{}: {}",
                                        mod_fluent::get_translation(
                                            &bundle,
                                            "update-fail-google-contacts"
                                        ),
                                        e
                                    );
                                    std::process::exit(1);
                                }
                            }
                        }
                        UpdateSource::FromAddressBook => {
                            // .addressbookに追加する
                            let mut existing_nicknames = Vec::new();
                            let nickname = generate_nickname(&gname, 1, &mut existing_nicknames);
                            apeople.push(APerson {
                                nickname,
                                name: gname,
                                email: email.to_owned(),
                                fcc: "".to_string(),
                                biography: gbiography,
                            });
                            apeople_diarty = true;
                        }
                    }
                }
            }

            for email in unique_to_apeople {
                // このメールアドレスは.addressbookにのみ存在し、Google Contactsには存在しない。
                if email.is_empty() {
                    continue;
                }

                // このメールアドレスを持つ人物を.addressbookから探す
                // remove_related_apersons関数との兼ね合いでapeople.clone()を渡す
                let apeople_clone = apeople.clone();
                let related_apeople = get_related_apersons(&apeople_clone, email);

                let mut source;
                for aperson in &related_apeople {
                    // Google Contactsに新規登録するか、.addressbookから削除するかを入力させる
                    println!(
                        "{}",
                        mod_fluent::get_translation(&bundle, "add-g-or-delete-a-mode")
                    );
                    println!(
                        ".addressbook   :{}/{}/{}/{}",
                        aperson.nickname, aperson.name, aperson.email, aperson.biography
                    );

                    source = input_select_source(&bundle);

                    // ユーザ入力に従って分岐
                    match source {
                        UpdateSource::FromGoogle => {
                            match update_google_contacts(None, &aperson, &service).await {
                                Ok(()) => {
                                    println!(
                                        "{}",
                                        mod_fluent::get_translation(
                                            &bundle,
                                            "update-success-google-contacts"
                                        )
                                    );
                                }
                                Err(e) => {
                                    eprintln!(
                                        "{}: {}",
                                        mod_fluent::get_translation(
                                            &bundle,
                                            "update-fail-google-contacts"
                                        ),
                                        e
                                    );
                                    std::process::exit(1);
                                }
                            }
                        }
                        UpdateSource::FromAddressBook => {
                            // .addressbookから削除する
                            remove_related_apersons(&mut apeople, &related_apeople);
                            apeople_diarty = true;
                        }
                    }
                }
            }

            for email in common_emails {
                // このメールアドレスは両者共通に存在する
                let apeople_clone = apeople.clone();
                let aperson = match apeople_clone.iter().find(|&ap| &ap.email == email) {
                    Some(s) => s,
                    None => continue,
                };

                // このメールアドレスを持つGoogle Contactsの要素だけループする
                let related_persons = get_related_gpersons(&gpersons, email);
                for person in related_persons {
                    // 名前を取得
                    let gname = get_gcontact_name(person);

                    // ニックネームを取得する
                    let gnickname = get_gcontact_nickname(person);

                    // メモを取得する
                    let gbiography = get_gcontact_biography(person);

                    // .addressbookに格納されているニックネームはそのまま使わず、
                    // 末尾の数字を取り除き、
                    // generate_nickname()で作ったニックネームと同じ場合はGoogle Contactsと同じとする
                    let mut anickname = split_string_and_number(&aperson.nickname).0;
                    let last_name_part = aperson
                        .name
                        .split_whitespace()
                        .last()
                        .unwrap_or("Unknown")
                        .to_string();
                    if anickname == last_name_part {
                        anickname = gnickname.clone();
                    }

                    if aperson.name == gname {
                        // メールアドレスと名前が同じ
                        if (anickname == gnickname) && (aperson.biography == gbiography) {
                            // ニックネームもメモも同じ
                            // 他のpersonのループを続ける
                            continue;
                        }
                    }

                    // ここまで来たらデータを更新する
                    let source;

                    // Google Contactsと.addressbookのどちらを優先するか入力させる
                    println!("{}", mod_fluent::get_translation(&bundle, "update-mode"));
                    println!("Google Contacts:{}/{}/{}", gname, gnickname, gbiography);
                    println!(
                        ".addressbook   :{}/{}/{}",
                        aperson.name, aperson.nickname, aperson.biography
                    );

                    source = input_select_source(&bundle);

                    // 既存の人物を更新する
                    match source {
                        UpdateSource::FromGoogle => {
                            // .addressbookを更新する
                            let fcc = aperson.clone().fcc;
                            let mut existing_nicknames = Vec::new();
                            let nickname = generate_nickname(&gname, 1, &mut existing_nicknames);
                            let aperson_clone = aperson.clone();
                            // .addressbookから該当する値を消す
                            remove_related_apersons(&mut apeople, &vec![&aperson_clone]);
                            apeople.push(APerson {
                                nickname,
                                name: gname,
                                email: email.to_owned(),
                                fcc, // フィールド名と変数名が同じ
                                biography: gbiography,
                            });
                            apeople_diarty = true;
                        }
                        UpdateSource::FromAddressBook => {
                            // Google Contactsを更新する
                            match update_google_contacts(Some(person), &aperson, &service).await {
                                Ok(()) => {
                                    println!(
                                        "{}",
                                        mod_fluent::get_translation(
                                            &bundle,
                                            "update-success-google-contacts"
                                        )
                                    );
                                }
                                Err(e) => {
                                    eprintln!(
                                        "{}: {}",
                                        mod_fluent::get_translation(
                                            &bundle,
                                            "update-fail-google-contacts"
                                        ),
                                        e
                                    );
                                    std::process::exit(1);
                                }
                            }
                        }
                    }
                }
            }

            if apeople_diarty {
                // apeopleを.addressbookに書き込む
                // CSVファイルライター（タブ区切り）を初期化
                let mut writer = WriterBuilder::new()
                    .delimiter(b'\t')
                    .from_path(addressbook_path)
                    .unwrap_or_else(|e| {
                        eprintln!(
                            "{}: {}",
                            mod_fluent::get_translation(&bundle, "init-error"),
                            e
                        );
                        std::process::exit(1);
                    });

                // 各apersonをCSVに書き込む
                let apeople_clone = apeople.clone();
                for aperson in apeople_clone {
                    if !aperson.email.is_empty() {
                        if let Err(e) = writer.write_record(&[
                            &aperson.nickname,
                            &aperson.name,
                            &aperson.email,
                            &aperson.fcc,
                            &aperson.biography,
                        ]) {
                            eprintln!(
                                "{}: {}",
                                mod_fluent::get_translation(&bundle, "write-error"),
                                e
                            );
                            std::process::exit(1);
                        }
                    }
                }

                // CSVファイルへの書き込みを完了
                if let Err(e) = writer.flush() {
                    eprintln!(
                        "{}: {}",
                        mod_fluent::get_translation(&bundle, "flush-error"),
                        e
                    );
                    std::process::exit(1);
                };

                // 書き込み完了メッセージを表示
                println!("{}", mod_fluent::get_translation(&bundle, "write-complete"));
            }
        }
    }
}
