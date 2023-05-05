//! API contract for `torrent` context.

/*
todo:

Delete torrent:

- After deleting a torrent, it should be removed from the tracker whitelist

Get torrent info:

- The torrent info:
    - should contain the tracker URL
        - If no user owned tracker key can be found, it should use the default tracker url
        - If    user owned tracker key can be found, it should use the personal tracker url
    - should contain the magnet link with the trackers from the torrent file
    - should contain realtime seeders and leechers from the tracker
*/

mod for_guests {
    use torrust_index_backend::utils::parse_torrent::decode_torrent;

    use crate::common::client::Client;
    use crate::common::contexts::category::fixtures::software_predefined_category_id;
    use crate::common::contexts::torrent::asserts::assert_expected_torrent_details;
    use crate::common::contexts::torrent::responses::{
        Category, File, TorrentDetails, TorrentDetailsResponse, TorrentListResponse,
    };
    use crate::e2e::contexts::torrent::asserts::expected_torrent;
    use crate::e2e::contexts::torrent::steps::upload_random_torrent_to_index;
    use crate::e2e::contexts::user::steps::new_logged_in_user;
    use crate::e2e::environment::TestEnv;

    #[tokio::test]
    async fn it_should_allow_guests_to_get_torrents() {
        let mut env = TestEnv::new();
        env.start().await;

        if !env.provides_a_tracker() {
            println!("test skipped. It requires a tracker to be running.");
            return;
        }

        let client = Client::unauthenticated(&env.server_socket_addr().unwrap());

        let uploader = new_logged_in_user(&env).await;
        let (_test_torrent, indexed_torrent) = upload_random_torrent_to_index(&uploader, &env).await;

        let response = client.get_torrents().await;

        let torrent_list_response: TorrentListResponse = serde_json::from_str(&response.body).unwrap();

        assert!(torrent_list_response.data.total > 0);
        assert!(torrent_list_response.data.contains(indexed_torrent.torrent_id));
        assert!(response.is_json_and_ok());
    }

    #[tokio::test]
    async fn it_should_allow_guests_to_get_torrent_details_searching_by_id() {
        let mut env = TestEnv::new();
        env.start().await;

        if !env.provides_a_tracker() {
            println!("test skipped. It requires a tracker to be running.");
            return;
        }

        let client = Client::unauthenticated(&env.server_socket_addr().unwrap());

        let uploader = new_logged_in_user(&env).await;
        let (test_torrent, uploaded_torrent) = upload_random_torrent_to_index(&uploader, &env).await;

        let response = client.get_torrent(uploaded_torrent.torrent_id).await;

        let torrent_details_response: TorrentDetailsResponse = serde_json::from_str(&response.body).unwrap();

        let expected_torrent = TorrentDetails {
            torrent_id: uploaded_torrent.torrent_id,
            uploader: uploader.username,
            info_hash: test_torrent.file_info.info_hash.to_uppercase(),
            title: test_torrent.index_info.title.clone(),
            description: test_torrent.index_info.description,
            category: Category {
                category_id: software_predefined_category_id(),
                name: test_torrent.index_info.category,
                num_torrents: 19, // Ignored in assertion
            },
            upload_date: "2023-04-27 07:56:08".to_string(), // Ignored in assertion
            file_size: test_torrent.file_info.content_size,
            seeders: 0,
            leechers: 0,
            files: vec![File {
                path: vec![test_torrent.file_info.files[0].clone()],
                // Using one file torrent for testing: content_size = first file size
                length: test_torrent.file_info.content_size,
                md5sum: None,
            }],
            // code-review: why is this duplicated?
            trackers: vec!["udp://tracker:6969".to_string(), "udp://tracker:6969".to_string()],
            magnet_link: format!(
                // cspell:disable-next-line
                "magnet:?xt=urn:btih:{}&dn={}&tr=udp%3A%2F%2Ftracker%3A6969&tr=udp%3A%2F%2Ftracker%3A6969",
                test_torrent.file_info.info_hash.to_uppercase(),
                test_torrent.index_info.title
            ),
        };

        assert_expected_torrent_details(&torrent_details_response.data, &expected_torrent);
        assert!(response.is_json_and_ok());
    }

    #[tokio::test]
    async fn it_should_allow_guests_to_download_a_torrent_file_searching_by_id() {
        let mut env = TestEnv::new();
        env.start().await;

        if !env.provides_a_tracker() {
            println!("test skipped. It requires a tracker to be running.");
            return;
        }

        let client = Client::unauthenticated(&env.server_socket_addr().unwrap());

        let uploader = new_logged_in_user(&env).await;
        let (test_torrent, torrent_listed_in_index) = upload_random_torrent_to_index(&uploader, &env).await;

        let response = client.download_torrent(torrent_listed_in_index.torrent_id).await;

        let torrent = decode_torrent(&response.bytes).unwrap();
        let uploaded_torrent = decode_torrent(&test_torrent.index_info.torrent_file.contents).unwrap();
        let expected_torrent = expected_torrent(uploaded_torrent, &env, &None).await;
        assert_eq!(torrent, expected_torrent);
        assert!(response.is_bittorrent_and_ok());
    }

    #[tokio::test]
    async fn it_should_not_allow_guests_to_delete_torrents() {
        let mut env = TestEnv::new();
        env.start().await;

        if !env.provides_a_tracker() {
            println!("test skipped. It requires a tracker to be running.");
            return;
        }

        let client = Client::unauthenticated(&env.server_socket_addr().unwrap());

        let uploader = new_logged_in_user(&env).await;
        let (_test_torrent, uploaded_torrent) = upload_random_torrent_to_index(&uploader, &env).await;

        let response = client.delete_torrent(uploaded_torrent.torrent_id).await;

        assert_eq!(response.status, 401);
    }
}

mod for_authenticated_users {

    use torrust_index_backend::utils::parse_torrent::decode_torrent;

    use crate::common::client::Client;
    use crate::common::contexts::torrent::fixtures::random_torrent;
    use crate::common::contexts::torrent::forms::UploadTorrentMultipartForm;
    use crate::common::contexts::torrent::responses::UploadedTorrentResponse;
    use crate::e2e::contexts::torrent::asserts::expected_torrent;
    use crate::e2e::contexts::torrent::steps::upload_random_torrent_to_index;
    use crate::e2e::contexts::user::steps::new_logged_in_user;
    use crate::e2e::environment::TestEnv;

    #[tokio::test]
    async fn it_should_allow_authenticated_users_to_upload_new_torrents() {
        let mut env = TestEnv::new();
        env.start().await;

        if !env.provides_a_tracker() {
            println!("test skipped. It requires a tracker to be running.");
            return;
        }

        let uploader = new_logged_in_user(&env).await;
        let client = Client::authenticated(&env.server_socket_addr().unwrap(), &uploader.token);

        let test_torrent = random_torrent();

        let form: UploadTorrentMultipartForm = test_torrent.index_info.into();

        let response = client.upload_torrent(form.into()).await;

        let _uploaded_torrent_response: UploadedTorrentResponse = serde_json::from_str(&response.body).unwrap();

        // code-review: the response only returns the torrent autoincrement ID
        // generated by the DB. So we can't assert that the torrent was uploaded.
        // We could return the infohash.
        // We are going to use the infohash to get the torrent. See issue:
        // https://github.com/torrust/torrust-index-backend/issues/115

        assert!(response.is_json_and_ok());
    }

    #[tokio::test]
    async fn it_should_not_allow_uploading_a_torrent_with_a_non_existing_category() {
        let mut env = TestEnv::new();
        env.start().await;

        let uploader = new_logged_in_user(&env).await;
        let client = Client::authenticated(&env.server_socket_addr().unwrap(), &uploader.token);

        let mut test_torrent = random_torrent();

        test_torrent.index_info.category = "non-existing-category".to_string();

        let form: UploadTorrentMultipartForm = test_torrent.index_info.into();

        let response = client.upload_torrent(form.into()).await;

        assert_eq!(response.status, 400);
    }

    #[tokio::test]
    async fn it_should_not_allow_uploading_a_torrent_with_a_title_that_already_exists() {
        let mut env = TestEnv::new();
        env.start().await;

        if !env.provides_a_tracker() {
            println!("test skipped. It requires a tracker to be running.");
            return;
        }

        let uploader = new_logged_in_user(&env).await;
        let client = Client::authenticated(&env.server_socket_addr().unwrap(), &uploader.token);

        // Upload the first torrent
        let first_torrent = random_torrent();
        let first_torrent_title = first_torrent.index_info.title.clone();
        let form: UploadTorrentMultipartForm = first_torrent.index_info.into();
        let _response = client.upload_torrent(form.into()).await;

        // Upload the second torrent with the same title as the first one
        let mut second_torrent = random_torrent();
        second_torrent.index_info.title = first_torrent_title;
        let form: UploadTorrentMultipartForm = second_torrent.index_info.into();
        let response = client.upload_torrent(form.into()).await;

        assert_eq!(response.body, "{\"error\":\"This torrent title has already been used.\"}");
        assert_eq!(response.status, 400);
    }

    #[tokio::test]
    async fn it_should_not_allow_uploading_a_torrent_with_a_infohash_that_already_exists() {
        let mut env = TestEnv::new();
        env.start().await;

        if !env.provides_a_tracker() {
            println!("test skipped. It requires a tracker to be running.");
            return;
        }

        let uploader = new_logged_in_user(&env).await;
        let client = Client::authenticated(&env.server_socket_addr().unwrap(), &uploader.token);

        // Upload the first torrent
        let first_torrent = random_torrent();
        let mut first_torrent_clone = first_torrent.clone();
        let first_torrent_title = first_torrent.index_info.title.clone();
        let form: UploadTorrentMultipartForm = first_torrent.index_info.into();
        let _response = client.upload_torrent(form.into()).await;

        // Upload the second torrent with the same infohash as the first one.
        // We need to change the title otherwise the torrent will be rejected
        // because of the duplicate title.
        first_torrent_clone.index_info.title = format!("{}-clone", first_torrent_title);
        let form: UploadTorrentMultipartForm = first_torrent_clone.index_info.into();
        let response = client.upload_torrent(form.into()).await;

        assert_eq!(response.status, 400);
    }

    #[tokio::test]
    async fn it_should_allow_authenticated_users_to_download_a_torrent_with_a_personal_announce_url() {
        let mut env = TestEnv::new();
        env.start().await;

        if !env.provides_a_tracker() {
            println!("test skipped. It requires a tracker to be running.");
            return;
        }

        // Given a previously uploaded torrent
        let uploader = new_logged_in_user(&env).await;
        let (test_torrent, torrent_listed_in_index) = upload_random_torrent_to_index(&uploader, &env).await;
        let torrent_id = torrent_listed_in_index.torrent_id;
        let uploaded_torrent = decode_torrent(&test_torrent.index_info.torrent_file.contents).unwrap();

        // And a logged in user who is going to download the torrent
        let downloader = new_logged_in_user(&env).await;
        let client = Client::authenticated(&env.server_socket_addr().unwrap(), &downloader.token);

        // When the user downloads the torrent
        let response = client.download_torrent(torrent_id).await;
        let torrent = decode_torrent(&response.bytes).unwrap();

        // Then the torrent should have the personal announce URL
        let expected_torrent = expected_torrent(uploaded_torrent, &env, &Some(downloader)).await;

        assert_eq!(torrent, expected_torrent);
        assert!(response.is_bittorrent_and_ok());
    }

    mod and_non_admins {
        use crate::common::client::Client;
        use crate::e2e::contexts::torrent::steps::upload_random_torrent_to_index;
        use crate::e2e::contexts::user::steps::new_logged_in_user;
        use crate::e2e::environment::TestEnv;

        #[tokio::test]
        async fn it_should_not_allow_non_admins_to_delete_torrents() {
            let mut env = TestEnv::new();
            env.start().await;

            if !env.provides_a_tracker() {
                println!("test skipped. It requires a tracker to be running.");
                return;
            }

            let uploader = new_logged_in_user(&env).await;
            let (_test_torrent, uploaded_torrent) = upload_random_torrent_to_index(&uploader, &env).await;

            let client = Client::authenticated(&env.server_socket_addr().unwrap(), &uploader.token);

            let response = client.delete_torrent(uploaded_torrent.torrent_id).await;

            assert_eq!(response.status, 403);
        }
    }

    mod and_torrent_owners {
        use crate::common::client::Client;
        use crate::common::contexts::torrent::forms::UpdateTorrentFrom;
        use crate::common::contexts::torrent::responses::UpdatedTorrentResponse;
        use crate::e2e::contexts::torrent::steps::upload_random_torrent_to_index;
        use crate::e2e::contexts::user::steps::new_logged_in_user;
        use crate::e2e::environment::TestEnv;

        #[tokio::test]
        async fn it_should_allow_torrent_owners_to_update_their_torrents() {
            let mut env = TestEnv::new();
            env.start().await;

            if !env.provides_a_tracker() {
                println!("test skipped. It requires a tracker to be running.");
                return;
            }

            let uploader = new_logged_in_user(&env).await;
            let (test_torrent, uploaded_torrent) = upload_random_torrent_to_index(&uploader, &env).await;

            let client = Client::authenticated(&env.server_socket_addr().unwrap(), &uploader.token);

            let new_title = format!("{}-new-title", test_torrent.index_info.title);
            let new_description = format!("{}-new-description", test_torrent.index_info.description);

            let response = client
                .update_torrent(
                    uploaded_torrent.torrent_id,
                    UpdateTorrentFrom {
                        title: Some(new_title.clone()),
                        description: Some(new_description.clone()),
                    },
                )
                .await;

            let updated_torrent_response: UpdatedTorrentResponse = serde_json::from_str(&response.body).unwrap();

            let torrent = updated_torrent_response.data;

            assert_eq!(torrent.title, new_title);
            assert_eq!(torrent.description, new_description);
            assert!(response.is_json_and_ok());
        }
    }

    mod and_admins {
        use crate::common::client::Client;
        use crate::common::contexts::torrent::forms::UpdateTorrentFrom;
        use crate::common::contexts::torrent::responses::{DeletedTorrentResponse, UpdatedTorrentResponse};
        use crate::e2e::contexts::torrent::steps::upload_random_torrent_to_index;
        use crate::e2e::contexts::user::steps::{new_logged_in_admin, new_logged_in_user};
        use crate::e2e::environment::TestEnv;

        #[tokio::test]
        async fn it_should_allow_admins_to_delete_torrents_searching_by_id() {
            let mut env = TestEnv::new();
            env.start().await;

            if !env.provides_a_tracker() {
                println!("test skipped. It requires a tracker to be running.");
                return;
            }

            let uploader = new_logged_in_user(&env).await;
            let (_test_torrent, uploaded_torrent) = upload_random_torrent_to_index(&uploader, &env).await;

            let admin = new_logged_in_admin(&env).await;
            let client = Client::authenticated(&env.server_socket_addr().unwrap(), &admin.token);

            let response = client.delete_torrent(uploaded_torrent.torrent_id).await;

            let deleted_torrent_response: DeletedTorrentResponse = serde_json::from_str(&response.body).unwrap();

            assert_eq!(deleted_torrent_response.data.torrent_id, uploaded_torrent.torrent_id);
            assert!(response.is_json_and_ok());
        }

        #[tokio::test]
        async fn it_should_allow_admins_to_update_someone_elses_torrents() {
            let mut env = TestEnv::new();
            env.start().await;

            if !env.provides_a_tracker() {
                println!("test skipped. It requires a tracker to be running.");
                return;
            }

            let uploader = new_logged_in_user(&env).await;
            let (test_torrent, uploaded_torrent) = upload_random_torrent_to_index(&uploader, &env).await;

            let logged_in_admin = new_logged_in_admin(&env).await;
            let client = Client::authenticated(&env.server_socket_addr().unwrap(), &logged_in_admin.token);

            let new_title = format!("{}-new-title", test_torrent.index_info.title);
            let new_description = format!("{}-new-description", test_torrent.index_info.description);

            let response = client
                .update_torrent(
                    uploaded_torrent.torrent_id,
                    UpdateTorrentFrom {
                        title: Some(new_title.clone()),
                        description: Some(new_description.clone()),
                    },
                )
                .await;

            let updated_torrent_response: UpdatedTorrentResponse = serde_json::from_str(&response.body).unwrap();

            let torrent = updated_torrent_response.data;

            assert_eq!(torrent.title, new_title);
            assert_eq!(torrent.description, new_description);
            assert!(response.is_json_and_ok());
        }
    }
}
