// Copyright 2023, Takahiro Yoshizawa
// Use of this source code is permitted under the license.
// The license can be viewed in the LICENSE file located in the project's top directory.

// Author: Takahiro Yoshizawa
// Description: A Rust program to process contact information using Google People API
// and export it to the AddressBook of Alpine Email Program.

use fluent::{bundle::FluentBundle, FluentResource}; // ローカライゼーション機能を提供するfluentクレート関連モジュール
use intl_memoizer::concurrent::IntlLangMemoizer; // 国際化機能を提供するintl_memoizerクレートのモジュール
use std::fs; // ファイルシステム操作のための標準ライブラリのモジュール

// intl_memoizer::concurrent::IntlLangMemoizerを型引数として指定
pub fn init_fluent_bundle(locale: &str) -> FluentBundle<FluentResource, IntlLangMemoizer> {
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
pub fn get_translation(bundle: &FluentBundle<FluentResource, IntlLangMemoizer>, message_id: &str) -> String {
    let message = bundle.get_message(message_id).expect("Message doesn't exist.");
    let pattern = message.value().expect("Message has no value.");
    let mut errors = vec![];
    bundle.format_pattern(&pattern, None, &mut errors).to_string()
}
