# gcontacts

## 概要
Google ContactsからAlpine Email Program( https://alpineapp.email/ )のAddressBookを取得します。
バージョン0.3.0から同期機能が追加されています。

## インストール方法
###Google Cloud Platformでのclient_secret.jsonの取得手順
#### Google Cloud Platformにログイン:
Google Cloud Platformのコンソールにアクセスし、Googleアカウントでログインします。

#### プロジェクトの作成:
コンソールのホームページで「プロジェクトを作成」をクリックします。
プロジェクト名を入力し、必要に応じて組織と場所を選択し、「作成」をクリックします。

#### APIとサービスを有効にする:
新しく作成したプロジェクトを選択し、「APIとサービス」ダッシュボードに移動します。
「ライブラリ」をクリックし、使用するAPI（この場合はGoogle People API）を検索して選択し、「有効にする」をクリックします。

#### 認証情報の設定:
APIダッシュボードの左側のメニューで「認証情報」を選択します。
「認証情報を作成」ボタンをクリックし、「OAuth クライアント ID」を選択します。

#### OAuth 同意画面の設定:
まだ設定していない場合は、OAuth 同意画面を設定します。アプリケーションの名前、サポートメール、承認されたドメインなどの情報を入力します。

#### OAuth クライアント IDの作成:
「アプリケーションの種類」を選択します（たとえば、ウェブアプリケーション、その他など）。
必要な情報を入力し、「作成」をクリックします。

#### client_secret.jsonのダウンロード:
認証情報ページに戻り、作成したOAuth 2.0 クライアント IDの右側にあるダウンロードボタン（ダウンロードアイコン）をクリックします。
これによりclient_secret.jsonファイルがダウンロードされます。

#### アプリケーションにclient_secret.jsonを統合:
ダウンロードしたclient_secret.jsonを、ホームディレクトリの.gcontacts/client_secret.jsonに配置します。

#### 注意事項
Google Cloud Platformでの作業は、課金が発生する可能性があるため、利用規約と料金に注意してください。
client_secret.jsonに含まれる情報は機密情報です。安全に管理し、公開リポジトリにアップロードしないようにしてください。
### ビルド方法
```
$ cargo build --release
```
## 使用方法(バージョン0.3.0以上)
### ~/.addressbookをGoogle Contactsのデータで初期化する
```
$ ./target/release/gcontacts init
```
### ~/.addressbookをGoogle Contactsと同期する
```
$ ./target/release/gcontacts sync
```
## ライセンス
このプロジェクトはApache License 2.0の下でライセンスされています。詳細はLICENSEファイルをご覧ください。

## 著者
Takahiro Yoshizawa
