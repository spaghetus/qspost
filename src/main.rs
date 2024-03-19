use clap::Parser;
use mastodon_async::{
	registration::Registered,
	scopes::{Scopes, Write},
	Data, Mastodon, NewStatus, Registration, Visibility,
};
use rocket::{
	get,
	http::{CookieJar, Status},
	request::{FromRequest, Outcome},
	response::Redirect,
	routes, Request, State,
};
use rocket_dyn_templates::{context, Template};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Parser)]
struct Args {
	/// The name of the program, as shown on the OAuth consent screen
	#[arg(short, long, default_value = "QSPost")]
	pub name: String,
	/// The base URL of the program, used to generate redirect URLs (should not end with a slash)
	#[arg(short, long, default_value = "http://localhost:8000")]
	pub base_url: String,
}

#[rocket::launch]
fn start() -> _ {
	let args = Args::parse();
	rocket::build()
		.manage(args)
		.attach(Template::fairing())
		.mount(
			"/",
			routes![
				entrypoint,
				start_login,
				finish_login,
				settings,
				settings_submit,
				post
			],
		)
}

#[derive(Serialize, Deserialize, Default)]
struct Settings {
	pub base_url: String,
	pub client_id: String,
	pub client_secret: String,
	pub token: Option<String>,
	pub post_privately: bool,
	pub tags: Vec<String>,
}

#[derive(Debug, Error)]
enum GetSettingsError {
	#[error("No settings cookie")]
	NoSettings,
	#[error("Json failed")]
	Json(#[from] serde_json::Error),
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for Settings {
	type Error = GetSettingsError;

	async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
		let cookie = match request.cookies().get_private("settings") {
			Some(v) => v,
			None => return Outcome::Error((Status::BadRequest, GetSettingsError::NoSettings)),
		};

		let settings = match serde_json::from_str(cookie.value()) {
			Ok(v) => v,
			Err(e) => {
				request.cookies().remove("settings");
				return Outcome::Error((Status::BadRequest, GetSettingsError::Json(e)));
			}
		};
		rocket::outcome::Outcome::Success(settings)
	}
}

#[get("/")]
fn entrypoint(settings: Option<Settings>) -> Template {
	Template::render("entry", context! {logged_in: settings.is_some()})
}

#[get("/?<instance>")]
async fn start_login(
	args: &State<Args>,
	cookies: &CookieJar<'_>,
	instance: &str,
) -> Result<Redirect, String> {
	let redirect = format!("{}/finish_login", args.base_url);
	let registered = match Registration::new(instance)
		.client_name(&args.name)
		.redirect_uris(redirect)
		.scopes(Scopes::write(Write::Statuses))
		.build()
		.await
	{
		Ok(r) => r,
		Err(e) => {
			eprintln!("{e}");
			return Err(e.to_string());
		}
	};

	let auth = match registered.authorize_url() {
		Ok(r) => r,
		Err(e) => {
			eprintln!("{e}");
			return Err(e.to_string());
		}
	};
	let (_, client_id, client_secret, _, _, _) = registered.into_parts();
	let settings = Settings {
		base_url: instance.to_string(),
		client_id,
		client_secret,
		token: None,
		post_privately: true,
		tags: vec![],
	};
	cookies.add_private(("settings", serde_json::to_string(&settings).unwrap()));

	Ok(Redirect::temporary(auth))
}

#[get("/finish_login?<code>")]
async fn finish_login(
	code: &str,
	settings: Settings,
	args: &State<Args>,
	cookies: &CookieJar<'_>,
) -> Result<Redirect, String> {
	let redirect = format!("{}/finish_login", args.base_url);
	let registered = Registered::from_parts(
		&settings.base_url,
		&settings.client_id,
		&settings.client_secret,
		&redirect,
		Scopes::write(Write::Statuses),
		false,
	);
	let mastodon = match registered.complete(code).await {
		Ok(m) => m,
		Err(e) => {
			eprintln!("{e}");
			return Err(e.to_string());
		}
	};
	let settings = Settings {
		token: Some(mastodon.data.token.to_string()),
		..settings
	};
	cookies.add_private(("settings", serde_json::to_string(&settings).unwrap()));

	Ok(Redirect::temporary("/settings"))
}

#[get("/settings")]
async fn settings(settings: Settings) -> Template {
	Template::render(
		"settings",
		context! {
			post_privately: settings.post_privately,
			tags: &settings.tags
		},
	)
}

#[get("/settings-submit?<post_privately>&<tags>")]
fn settings_submit(
	post_privately: bool,
	tags: String,
	cookies: &CookieJar<'_>,
	mut settings: Settings,
) -> Redirect {
	settings.post_privately = post_privately;
	settings.tags = tags
		.split_ascii_whitespace()
		.filter(|s| s.starts_with('#'))
		.map(|s| s.to_string())
		.collect::<Vec<_>>();
	cookies.add_private(("settings", serde_json::to_string(&settings).unwrap()));

	Redirect::temporary("/settings")
}

#[get("/post?<body>&<private>")]
async fn post(
	settings: Settings,
	body: String,
	private: Option<bool>,
	args: &State<Args>,
) -> Result<Redirect, (Status, String)> {
	let redirect = format!("{}/finish_login", args.base_url);
	let private = private.unwrap_or(settings.post_privately);

	let Some(token) = settings.token else {
		return Ok(Redirect::temporary("/"));
	};

	let mastodon = Mastodon::from(Data {
		base: settings.base_url.into(),
		client_id: settings.client_id.into(),
		client_secret: settings.client_secret.into(),
		redirect: redirect.into(),
		token: token.into(),
	});

	let body = format!("{body}\n\n{}", settings.tags.join(" ")).to_string();

	let status = match mastodon
		.new_status(NewStatus {
			status: Some(body),
			visibility: Some(if private {
				Visibility::Direct
			} else {
				Visibility::Unlisted
			}),
			..Default::default()
		})
		.await
	{
		Ok(s) => s,
		Err(e) => {
			eprintln!("{e}");
			return Err((Status::FailedDependency, e.to_string()));
		}
	};

	Ok(Redirect::temporary(status.uri))
}
