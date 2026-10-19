use clap::Parser;
use frame_buffer_wasmtime::{FrameBufferCtx, FrameBufferCtxView};
use std::sync::Arc;
use surface_wasmtime::{
    SurfaceCtx, SurfaceCtxView, SurfaceFrameBufferCtx, SurfaceFrameBufferCtxView, SurfaceWebgpuCtx,
    SurfaceWebgpuCtxView,
};
use wasi_webgpu_wasmtime::{WasiWebGpuCtx, WasiWebGpuCtxView, WasiWebGpuOptions};
use wasmtime::{
    component::{Component, Linker},
    error::Context,
    Config, Engine, Store,
};

use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxBuilder, WasiView};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    file: String,
}

struct HostState {
    pub table: ResourceTable,
    pub ctx: WasiCtx,
    pub instance: Arc<wasi_webgpu_wasmtime::reexports::wgpu_core::global::Global>,
    pub webgpu_options: WasiWebGpuOptions,
    pub main_thread_proxy: surface_wasmtime::winit::WasiWinitEventLoopProxy,
}

impl HostState {
    fn new(main_thread_proxy: surface_wasmtime::winit::WasiWinitEventLoopProxy) -> Self {
        Self {
            table: ResourceTable::new(),
            ctx: WasiCtxBuilder::new().inherit_stdio().build(),
            instance: Arc::new(wasi_webgpu_wasmtime::reexports::wgpu_core::global::Global::new(
                "webgpu",
                wasi_webgpu_wasmtime::reexports::wgpu_types::InstanceDescriptor {
                    backends: wasi_webgpu_wasmtime::reexports::wgpu_types::Backends::all(),
                    flags: wasi_webgpu_wasmtime::reexports::wgpu_types::InstanceFlags::from_build_config(),
                    backend_options: wasi_webgpu_wasmtime::reexports::wgpu_types::BackendOptions::default(),
                    memory_budget_thresholds: wasi_webgpu_wasmtime::reexports::wgpu_types::MemoryBudgetThresholds::default(),
                    display: None,
                },
                None,
            )),
            webgpu_options: WasiWebGpuOptions::default(),
            main_thread_proxy,
        }
    }
}

impl WasiView for HostState {
    fn ctx(&mut self) -> wasmtime_wasi::WasiCtxView<'_> {
        wasmtime_wasi::WasiCtxView {
            ctx: &mut self.ctx,
            table: &mut self.table,
        }
    }
}

impl WasiWebGpuCtxView for HostState {
    fn webgpu_ctx(&mut self) -> WasiWebGpuCtx<'_> {
        WasiWebGpuCtx {
            instance: &self.instance,
            table: &mut self.table,
            options: &self.webgpu_options,
        }
    }
}

impl FrameBufferCtxView for HostState {
    fn frame_buffer_ctx<'a>(&'a mut self) -> FrameBufferCtx<'a> {
        FrameBufferCtx {
            table: &mut self.table,
        }
    }
}

impl SurfaceCtxView for HostState {
    type Spawner = surface_wasmtime::winit::WasiWinitEventLoopProxy;
    fn surface_ctx(&mut self) -> SurfaceCtx<'_, surface_wasmtime::winit::WasiWinitEventLoopProxy> {
        SurfaceCtx {
            table: &mut self.table,
            main_thread_spawner: &self.main_thread_proxy,
        }
    }
}

impl SurfaceWebgpuCtxView for HostState {
    type Spawner = surface_wasmtime::winit::WasiWinitEventLoopProxy;
    fn surface_webgpu_ctx(
        &mut self,
    ) -> SurfaceWebgpuCtx<'_, surface_wasmtime::winit::WasiWinitEventLoopProxy> {
        SurfaceWebgpuCtx {
            table: &mut self.table,
            instance: &self.instance,
            main_thread_spawner: &self.main_thread_proxy,
        }
    }
}

impl SurfaceFrameBufferCtxView for HostState {
    type Spawner = surface_wasmtime::winit::WasiWinitEventLoopProxy;
    fn surface_frame_buffer_ctx(
        &mut self,
    ) -> SurfaceFrameBufferCtx<'_, surface_wasmtime::winit::WasiWinitEventLoopProxy> {
        SurfaceFrameBufferCtx {
            table: &mut self.table,
            instance: &self.instance,
            main_thread_spawner: &self.main_thread_proxy,
        }
    }
}

#[tokio::main]
async fn main() -> wasmtime::Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    let args = Args::parse();

    let mut config = Config::default();
    config.wasm_component_model(true);
    let engine = Engine::new(&config)?;
    let mut linker: Linker<HostState> = Linker::new(&engine);

    wasi_webgpu_wasmtime::add_to_linker(&mut linker)?;
    frame_buffer_wasmtime::add_to_linker(&mut linker)?;
    surface_wasmtime::add_all_to_linker(&mut linker)?;
    wasmtime_wasi::p3::add_to_linker(&mut linker)?;

    let (main_thread_loop, main_thread_proxy) =
        surface_wasmtime::winit::create_wasi_winit_event_loop();
    let host_state = HostState::new(main_thread_proxy);

    let mut store = Store::new(&engine, host_state);

    let component =
        Component::from_file(&engine, &args.file).context("Component file not found")?;

    let command =
        wasmtime_wasi::p3::bindings::Command::instantiate_async(&mut store, &component, &linker)
            .await
            .unwrap();

    tokio::spawn(async move {
        store
            .run_concurrent(async move |store| command.wasi_cli_run().call_run(store).await)
            .await
            .expect("failed to run component")
            .expect("failed to invoke `run` function")
            .expect("`run` returned an error");
    });

    main_thread_loop.run();

    Ok(())
}
