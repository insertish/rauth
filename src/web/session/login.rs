/// Login to an account
/// POST /session/login
use rocket::serde::json::Json;
use rocket::State;

use crate::entities::*;
use crate::logic::Auth;
use crate::util::{Error, Result};

/* #[derive(Serialize, Deserialize, Debug)]
pub enum LoginType {
    Email,
    Password { password: String },
    SecurityKey { challenge: String },
} */

#[derive(Serialize, Deserialize)]
pub struct Data {
    pub email: String,

    pub password: Option<String>,
    pub challenge: Option<String>,

    // * Can't get this to work :(
    // #[serde(flatten)]
    // pub login_type: LoginType,
    pub friendly_name: Option<String>,
    pub captcha: Option<String>,
}

// TODO: remove dead_code
#[allow(dead_code)]
#[derive(Serialize)]
#[serde(tag = "result")]
pub enum Response {
    Success(Session),
    EmailOTP,
    MFA {
        ticket: String,
        // TODO: swap this out for an enum
        allowed_methods: Vec<String>,
    },
}

#[post("/login", data = "<data>")]
pub async fn login(auth: &State<Auth>, data: Json<Data>) -> Result<Json<Response>> {
    let data = data.into_inner();

    // Perform validation on given data.
    auth.check_captcha(data.captcha).await?;
    auth.validate_email(&data.email).await?;

    // Generate a session name ahead of time.
    let name = data.friendly_name.unwrap_or_else(|| "Unknown".to_string());

    // * We could check if passwords are compromised
    // * on login, in the future.
    // auth.validate_password(&password).await?;

    // Try to find the account we want.
    if let Some(account) = Account::find_one(&auth.db, doc! { "email": data.email }, None)
        .await
        .map_err(|_| Error::DatabaseError {
            operation: "find_one",
            with: "account",
        })?
    {
        // Figure out whether we are doing password, 1FA key or email 1FA OTP.
        if let Some(password) = data.password {
            if !argon2::verify_encoded(&account.password, password.as_bytes())
                // To prevent user enumeration, we should ignore
                // the error and pretend the password is wrong.
                .map_err(|_| Error::InvalidCredentials)?
            {
                return Err(Error::InvalidCredentials);
            }

            Ok(Json(Response::Success(
                auth.create_session(&account, name).await?,
            )))
        } else if let Some(_challenge) = data.challenge {
            // TODO: implement; issue #5
            Err(Error::InvalidCredentials)
        } else {
            // TODO: implement; issue #5
            Err(Error::InvalidCredentials)
        }
    } else {
        Err(Error::InvalidCredentials)
    }
}

#[cfg(test)]
mod tests {
    use crate::test::*;

    #[cfg(feature = "async-std-runtime")]
    #[async_std::test]
    async fn success() {
        let (_, auth) = for_test("login::success").await;

        auth.create_account("example@validemail.com".into(), "password".into(), false)
            .await
            .unwrap();

        let client =
            bootstrap_rocket_with_auth(auth, routes![crate::web::session::login::login]).await;

        let res = client
            .post("/login")
            .header(ContentType::JSON)
            .body(
                json!({
                    "email": "example@validemail.com",
                    "password": "password"
                })
                .to_string(),
            )
            .dispatch()
            .await;

        assert_eq!(res.status(), Status::Ok);
        assert!(serde_json::from_str::<Session>(&res.into_string().await.unwrap()).is_ok());
    }

    #[cfg(feature = "async-std-runtime")]
    #[async_std::test]
    async fn invalid_user() {
        let client = bootstrap_rocket(
            "create_account",
            "invalid_user",
            routes![crate::web::session::login::login],
        )
        .await;

        let res = client
            .post("/login")
            .header(ContentType::JSON)
            .body(
                json!({
                    "email": "example@validemail.com",
                    "password": "password"
                })
                .to_string(),
            )
            .dispatch()
            .await;

        assert_eq!(res.status(), Status::Unauthorized);
        assert_eq!(
            res.into_string().await,
            Some("{\"type\":\"InvalidCredentials\"}".into())
        );
    }
}