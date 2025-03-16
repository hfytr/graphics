use ash::{
    Device, Entry, Instance,
    ext::debug_utils,
    khr::{surface, swapchain},
    vk,
};
use std::{ffi::CStr, os::raw::c_char};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    raw_window_handle::{HasDisplayHandle, HasWindowHandle},
    window::{Window, WindowAttributes},
};

struct App {
    entry: Entry,
    instance: Instance,
    window: Window,
    pdevice: vk::PhysicalDevice,
    device: Device,
    queue: vk::Queue,
    surface: vk::SurfaceKHR,
    surface_loader: surface::Instance,
    swapchain: vk::SwapchainKHR,
    swap_device: swapchain::Device,
    swap_imgs: Vec<vk::Image>,
    swap_img_views: Vec<vk::ImageView>,
    format: vk::SurfaceFormatKHR,
    extent: vk::Extent2D,
}
struct WrappedApp(Option<App>);

impl ApplicationHandler for WrappedApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = event_loop
            .create_window(
                WindowAttributes::default()
                    .with_inner_size(winit::dpi::LogicalSize::new(800.0, 600.0))
                    .with_title("gaem"),
            )
            .unwrap();
        let entry = ash::Entry::linked();
        let instance = {
            let layer_names = [c"VK_LAYER_KHRONOS_validation"];
            let layer_names_raw: Vec<*const c_char> = layer_names
                .iter()
                .map(|raw_name| raw_name.as_ptr())
                .collect();
            let app_info = vk::ApplicationInfo::default()
                .api_version(vk::make_api_version(0, 1, 0, 0))
                .application_name(c"gaem")
                .application_version(0)
                .engine_name(c"gaem")
                .engine_version(0);
            let mut extension_names = ash_window::enumerate_required_extensions(
                window.display_handle().unwrap().as_raw(),
            )
            .expect("Failed to enumerate required extensions.")
            .to_vec();
            extension_names.push(debug_utils::NAME.as_ptr());
            let instance_create_info = vk::InstanceCreateInfo::default()
                .application_info(&app_info)
                .enabled_layer_names(&layer_names_raw)
                .enabled_extension_names(&extension_names)
                .flags(vk::InstanceCreateFlags::default());
            unsafe { entry.create_instance(&instance_create_info, None).unwrap() }
        };
        let surface = unsafe {
            ash_window::create_surface(
                &entry,
                &instance,
                window.display_handle().unwrap().as_raw(),
                window.window_handle().unwrap().as_raw(),
                None,
            )
            .unwrap()
        };
        let surface_loader = surface::Instance::new(&entry, &instance);

        let get_swap_support = |pdevice: vk::PhysicalDevice| unsafe {
            (
                surface_loader
                    .get_physical_device_surface_capabilities(pdevice, surface)
                    .expect("Failed to get device surface capabilities."),
                surface_loader
                    .get_physical_device_surface_formats(pdevice, surface)
                    .expect("Failed to get device surface formats."),
                surface_loader
                    .get_physical_device_surface_present_modes(pdevice, surface)
                    .expect("Failed to get device surface modes."),
            )
        };
        let extension_names = [swapchain::NAME];
        let extension_names_raw = [swapchain::NAME.as_ptr()];
        let check_dev_props_valid = |props: &Vec<vk::ExtensionProperties>| {
            extension_names.iter().all(|e| {
                props
                    .into_iter()
                    .map(|p| p.extension_name_as_c_str().unwrap())
                    .collect::<Vec<&CStr>>()
                    .contains(e)
            })
        };
        let (queue_ind, pdevice) = unsafe {
            let check_device = |d: vk::PhysicalDevice, i: u32, info: &vk::QueueFamilyProperties| {
                info.queue_flags.contains(vk::QueueFlags::GRAPHICS)
                        && surface_loader
                            .get_physical_device_surface_support(d, i, surface)
                            .unwrap_or(false)
                        && instance
                            .enumerate_device_extension_properties(d)
                            .map(|props| check_dev_props_valid(&props))
                            .unwrap_or(false)
                        // only query support after verifying extensions
                        && {
                            let (_, formats, modes) = get_swap_support(d);
                            !formats.is_empty() && !modes.is_empty()
                        }
            };
            instance
                .enumerate_physical_devices()
                .expect("Failed to enumerate devices.")
                .iter()
                .find_map(|d| {
                    instance
                        .get_physical_device_queue_family_properties(*d)
                        .iter()
                        .enumerate()
                        .find_map(|(i, info)| {
                            check_device(*d, i as u32, info).then(|| (i as u32, *d))
                        })
                })
                .expect("Failed to find suitable device")
        };
        let device = unsafe {
            let q_create_infos = [vk::DeviceQueueCreateInfo::default()
                .queue_priorities(&[1.0])
                .queue_family_index(queue_ind)];
            let features = instance.get_physical_device_features(pdevice);
            let device_create_info = vk::DeviceCreateInfo::default()
                .enabled_features(&features)
                .enabled_extension_names(&extension_names_raw)
                .queue_create_infos(&q_create_infos);
            instance
                .create_device(pdevice, &device_create_info, None)
                .expect("Failed to create device.")
        };
        let queue = unsafe { device.get_device_queue(queue_ind, 0) };
        let swap_device = swapchain::Device::new(&instance, &device);
        let (swapchain, format, extent) = unsafe {
            let (capabilities, formats, modes) = get_swap_support(pdevice);
            let format = *formats
                .iter()
                .find(|f| {
                    f.format == vk::Format::B8G8R8A8_SRGB
                        && f.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
                })
                .unwrap_or(&formats[0]);
            let mode = *modes
                .iter()
                .find(|m| **m == vk::PresentModeKHR::MAILBOX)
                .unwrap_or(&vk::PresentModeKHR::FIFO);
            let extent = if capabilities.current_extent.width == u32::MAX {
                capabilities.max_image_extent
            } else {
                capabilities.current_extent
            };
            let img_count = if capabilities.max_image_count == 0
                || capabilities.max_image_count > capabilities.min_image_count
            {
                capabilities.min_image_count + 1
            } else {
                capabilities.max_image_count
            };
            let create_info = vk::SwapchainCreateInfoKHR::default()
                .image_format(format.format)
                .present_mode(mode)
                .min_image_count(img_count)
                .image_extent(extent)
                .image_color_space(format.color_space)
                .image_array_layers(1)
                .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
                .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
                .queue_family_indices(&[])
                .pre_transform(capabilities.current_transform)
                .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
                .clipped(true)
                .surface(surface)
                .old_swapchain(vk::SwapchainKHR::null());
            (
                swap_device
                    .create_swapchain(&create_info, None)
                    .expect("Failed to create swapchain."),
                format,
                extent,
            )
        };
        let swap_imgs = unsafe {
            swap_device
                .get_swapchain_images(swapchain)
                .expect("Failed to retrieve swapchain image handles.")
        };
        let swap_img_views = swap_imgs
            .iter()
            .map(|i| {
                let create_info = vk::ImageViewCreateInfo::default()
                    .image(*i)
                    .format(format.format)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .components(
                        vk::ComponentMapping::default()
                            .r(vk::ComponentSwizzle::IDENTITY)
                            .g(vk::ComponentSwizzle::IDENTITY)
                            .b(vk::ComponentSwizzle::IDENTITY)
                            .a(vk::ComponentSwizzle::IDENTITY),
                    )
                    .subresource_range(
                        vk::ImageSubresourceRange::default()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .base_mip_level(0)
                            .level_count(1)
                            .base_array_layer(0)
                            .layer_count(1),
                    );
                unsafe {
                    device
                        .create_image_view(&create_info, None)
                        .expect("Failed to create image view.")
                }
            })
            .collect();
        *self = WrappedApp(Some(App {
            entry,
            instance,
            window,
            device,
            pdevice,
            queue,
            surface,
            surface_loader,
            swap_device,
            swapchain,
            swap_imgs,
            swap_img_views,
            format,
            extent,
        }))
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _: winit::window::WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::Destroyed => event_loop.exit(),
            _ => {}
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        unsafe {
            self.swap_device.destroy_swapchain(self.swapchain, None);
            self.surface_loader.destroy_surface(self.surface, None);
            self.instance.destroy_instance(None);
            self.device.destroy_device(None);
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = WrappedApp(None);
    event_loop.run_app(&mut app).expect("Failed to run app.");
}
