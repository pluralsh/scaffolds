# ElasticSearch Post-Install

Elasticsearch doesn't support setting index policies and index lifecycle policies via CRDs in their community license.  To establish these, simply go to kibana and run the following API calls in the console:

```sh
PUT _ilm/policy/plrl-logs
{
  "policy": {
    "phases": {
      "warm": {
        "actions": {
          "forcemerge": {
            "max_num_segments": 1
          }
        },
        "min_age": "10d"
      },
      "delete": {
        "min_age": "30d",
        "actions": {
          "delete": {} 
        }
      }
    }
  }
}

PUT _index_template/plrl-logs
{
  "index_patterns": [
    "plrl-logs*"
  ],
  "template": {
    "settings": {
      "index": {
        "number_of_shards": 15,
        "refresh_interval": "30s"
      },
      "lifecycle": {
        "name": "plrl-logs"
      }
    }
  }
}
```

This will set up a sensible log rotation policy, but if you want to tune it, feel free (for instance you might be willing to do with less than 30d of retention).

## ECK Configuration

Elasticsearch's operator is actually very full-featured, if you'd like to tune your install we'd highly recommend checking out their docs at https://www.elastic.co/guide/en/cloud-on-k8s

For instance, to configure the node topology of your cluster, some useful guidance is given [here](https://www.elastic.co/guide/en/cloud-on-k8s/current/k8s-node-configuration.html)