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

/// システムの環境変数からロケール設定を取得する

use std::env; // 環境変数を扱うための 'env' モジュールをインポート

/// 環境変数からロケール設定を取得する
pub fn get_locale_from_env() -> String {
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

/// ロケールコードの形式が有効かどうかをチェックするヘルパー関数
fn is_valid_locale_format(code: &str) -> bool {
    // ロケールコードを '-' で分割して部分文字列のベクトルを生成
    let parts: Vec<&str> = code.split('-').collect();
    // ロケールコードが2つの部分から成り、各部分が英数字のみで構成されているかをチェック
    parts.len() == 2 && parts.iter().all(|&part| part.chars().all(|c| c.is_alphanumeric()))
}
