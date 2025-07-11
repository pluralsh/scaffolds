# Plural Airbyte

This is a baseline, prod ready airbyte installation using Plural.  It includes a few main components:

* S3/GCS/etc to handle blob storage.  Airbyte uses this to manage sync job logs among a few other things
* RDS/Google Cloud Sql, Azure Flexible Server to handle postgres. This gives you a robust RDBMS service to hold airbyte's core transactional data.
* Plural OIDC to handle authentication to Airbyte. Airbyte does not support this natively, and so we use oauth-proxy as a middleware to handle authentication.

In addition, there are a few common customizations you might want to do.

## Configure Basic Auth

Basic Auth for your airbyte instance is configured by default alongside the OpenID Connect setup brokered by Plural. The
basic auth user is hardfixed to `airbyte` and the random-generated password can be found in the stack outputs of your
generated airbyte stack, which should be named something like `airbyte-{cluster-name}`


## Contributing

If there are any features or documentation you'd like to add to this setup, please feel free to contribute back at https://github.com/pluralsh/scaffolds 
