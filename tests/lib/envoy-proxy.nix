# Envoy in front of the local daemon. gRPC alone auto-sizes its windows, so
# some hangs only show through Envoy. The windows are the HTTP/2 minimum, so
# less data has to be in flight for them to fill.
{ lib, pkgs, ... }:
{
  services.envoy = {
    enable = true;
    package = lib.mkDefault pkgs.envoy-bin;
    settings.static_resources = {
      listeners = [
        {
          name = "proxy";
          address.socket_address = {
            address = "127.0.0.1";
            port_value = 50060;
          };
          filter_chains = [
            {
              filters = [
                {
                  name = "envoy.filters.network.http_connection_manager";
                  typed_config = {
                    "@type" = "type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager";
                    stat_prefix = "proxy";
                    codec_type = "AUTO";
                    http2_protocol_options = {
                      initial_stream_window_size = 65535;
                      initial_connection_window_size = 65535;
                    };
                    stream_idle_timeout = "0s";
                    route_config.virtual_hosts = [
                      {
                        name = "all";
                        domains = [ "*" ];
                        routes = [
                          {
                            match.prefix = "/";
                            route = {
                              cluster = "daemon";
                              timeout = "0s";
                            };
                          }
                        ];
                      }
                    ];
                    http_filters = [
                      {
                        name = "envoy.filters.http.router";
                        typed_config."@type" = "type.googleapis.com/envoy.extensions.filters.http.router.v3.Router";
                      }
                    ];
                  };
                }
              ];
            }
          ];
        }
      ];
      clusters = [
        {
          name = "daemon";
          type = "STATIC";
          typed_extension_protocol_options."envoy.extensions.upstreams.http.v3.HttpProtocolOptions" = {
            "@type" = "type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions";
            explicit_http_config.http2_protocol_options = {
              initial_stream_window_size = 65535;
              initial_connection_window_size = 65535;
            };
          };
          load_assignment = {
            cluster_name = "daemon";
            endpoints = [
              {
                lb_endpoints = [
                  {
                    endpoint.address.socket_address = {
                      address = "127.0.0.1";
                      port_value = 50051;
                    };
                  }
                ];
              }
            ];
          };
        }
      ];
    };
  };
}
