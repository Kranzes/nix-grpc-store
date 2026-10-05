// NarFetcher against an in-process server whose FetchNars always fails.

#include <atomic>
#include <cassert>
#include <memory>
#include <string>

#include <grpcpp/channel.h>
#include <grpcpp/grpcpp.h>
#include <grpcpp/security/credentials.h>
#include <grpcpp/security/server_credentials.h>
#include <grpcpp/support/status.h>

#include <nix/store/path.hh>
#include <nix/util/error.hh>
#include <nix/util/serialise.hh>

#include "nar-fetcher.hh"
#include "nix_remote.grpc.pb.h"
#include "nix_remote.pb.h"

namespace {

class FailingService final : public nix::remote::NixRemote::Service
{
public:
    std::atomic<int> calls = 0;

    auto FetchNars(
        grpc::ServerContext * /*ctx*/,
        const nix::remote::FetchNarsRequest * /*request*/,
        grpc::ServerWriter<nix::remote::NarFrame> * /*writer*/) -> grpc::Status override
    {
        calls++;
        return {grpc::StatusCode::UNAVAILABLE, "worker draining"};
    }
};

auto fetchFails(nixgrpc::NarFetcher & fetcher, const nix::StorePath & path) -> bool
{
    nix::StringSink sink;
    try {
        fetcher.fetchInto(path, sink);
    } catch (nix::Error &) {
        return true;
    }
    return false;
}

// A failed fetch must not be cached: nix repl keeps the store after ^C and
// asks for the same path again.
void failedFetchIsRetried()
{
    FailingService service;
    grpc::ServerBuilder builder;
    int port = 0;
    builder.AddListeningPort("127.0.0.1:0", grpc::InsecureServerCredentials(), &port);
    builder.RegisterService(&service);
    auto const server = builder.BuildAndStart();
    assert(server);

    auto channel = grpc::CreateChannel("127.0.0.1:" + std::to_string(port), grpc::InsecureChannelCredentials());
    nixgrpc::NarFetcher fetcher([&] -> std::shared_ptr<grpc::Channel> { return channel; }, "test", 1);
    nix::StorePath const path("00000000000000000000000000000000-x");

    assert(fetchFails(fetcher, path));
    assert(fetchFails(fetcher, path));
    assert(service.calls == 2);
}

} // namespace

auto main() -> int
try {
    failedFetchIsRetried();
    return 0;
} catch (...) {
    return 1;
}
