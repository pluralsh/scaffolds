# Plural Airbyte

This is a baseline, prod ready airbyte installation using Plural.  It includes a few main components:

* S3/GCS/etc to handle blob storage.  Airbyte uses this to manage sync job logs among a few other things
* RDS/Google Cloud Sql, Azure Flexible Server to handle postgres. This gives you a robust RDBMS service to hold airbyte's core transactional data.
* Plural OIDC to handle authentication to Airbyte. Airbyte does not support this natively, and so we use oauth-proxy as a middleware to handle authentication.

In addtion, there are a few common customizations you might want to do.

## Configure Basic Auth

Basic auth allows you to set fixed usernames and passwords to pass to oauth-proxy for authentication.

## Configuring Basic Auth

Airbyte's api and web interface is not authenticated by default.  We provide an oauth proxy by default to grant some security to your airbyte install, but in order to integrate with tools like airflow, you'll likely want a means to authenticate with static creds.  That's where basic auth can be very useful.  The process is very simple.

First, you'll want to generate a random password, you can use the `plural` cli for this:

```sh
plural crypto random
```

Then you will create a `basicAuth` secret in the Plural UI for the airbyte service that was created (will be something like `airbyte-{cluster}`).  It will need to be a JSON-encoded map like:

```json
{"<user>": "<password>","<user2>": "<password2>"}
```

Your airbyte installation has already been configured to be able to read that secret and configure basic auth via helm.

Once you've completed the steps above to configure basic auth, you should be able to make api requests to your Airbyte 
instance accordingly:

```python
    # python

    import base64
    import requests

    user = "<insert-your-username>" # configured in previous step
    password = "<insert-your-password>" # configured in previous step
    base_url = "<insert-your-base-url>" # can be found in your project's context.yaml (spec.configuration.airbyte.hostname)
    credentials = f"{user}:{password}"
    credentials_base64 = base64.b64encode(credentials.encode("utf-8")).decode("utf-8")
    response = requests.post(
        url=f"https://{base_url}/api/v1/workspaces/list",
        headers={
            "accept": "application/json",
            "authorization": f"Basic {credentials_base64}"
        }
    )
    print(response.json())
```

```bash
    user="<insert-your-username>"  # configured in previous step
    password="<insert-your-password>"  # configured in previous step
    
    # Your base URL (can be found in your project's context.yaml - spec.configuration.airbyte.hostname)
    base_url="<insert-your-base-url>"
    
    # Combine the username and password with a colon (required for Basic Authentication)
    credentials="${user}:${password}"
    
    # Encode the credentials in base64
    credentials_base64=$(echo -n "$credentials" | base64)
    
    # Make an HTTP POST request using curl
    curl -X POST "https://${base_url}/api/v1/workspaces/list" \
        -H "accept: application/json" \
        -H "authorization: Basic $credentials_base64"
```

It's also worth noting that the [Airbyte Public API Docs](https://airbyte-public-api-docs.s3.us-east-2.amazonaws.com/) 
will serve as a more accurate reference than the [Airbyte Reference API Docs](https://reference.airbyte.com/reference/start) 
when building your application.


## Contributing

If there are any features or documentation you'd like to add to this setup, please feel free to contribute back at https://github.com/pluralsh/scaffolds 