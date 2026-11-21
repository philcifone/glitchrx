# Brief for Glitch Art application

Can you write a just a brief, not a SPEC for a web application/regular application for a computer, Linux and all other major OS compatible, in which I can use the tools like the ones Audacity has to create glitch art. In essence, I want to create Photoshop for glitch art. And I know that's lofty, but I know these tools exist.

## background

I used to simply upload a JPEG as raw data, avoid altering the header, and use a certain selection of effects to generate glitch art from photography. I forget which ones actually work effectively, like echo, reverb, delay, etc. And then I would save it as either Little endian or Big endian, etc. I do not know the difference. I do not know what they mean, but it was important. (And I anticipate this application choosing sensible defaults for export.) And then I would Have to add the .jpg extension back to the file and reopen the image to see the results. The gap was not seeing what I was doing to the image when I placed the effect. And I would like to be able to visualize it. I've used web applications to do pixel sorting, but I don't like uploading an image to someone else's website. But I really liked seeing like the visual of it all. And being able to just open up a Photoshop or a Darktable-like application to accomplish this would be really cool.

## goals & phases

I'll try to phase this out for you, but feel free to reorganize if the scaffolding and the structure demands. Each phase should end in a workable tool, and each phase afterwards should add more function and usability on top of it.

### phase 1

to start, We can omit a GUI and use a CLI tool that I've yet to name, in which I can point at a JPEG file with an option flag that designates a specific effect.

Usage example: glitch --echo example.jpg example-glitch.jpg

We should have at least 5 effects available to start with a --help flag that lists them and gives a usage example.

I want a sensible default workflow that refuses to overwrite an original file, and preserves user data. If the user does not add an option to rename a file or create a new file, it should automatically create a new file with the -glitch before the extension. Any further iterations on that same image should increment the number after glitch. So you have -glitch, the next one will be -glitch 1, -glitch 2, etc. an option to --reverse and do the math of whatever the effect did backwards to put the image back in place might be cool.

This first phase, we can keep things simple (if it matters) and only use JPEG. If this first step permits, and it's easy to do so, we can integrate TIFF, RAW, WebP, PNG, AVIF handling here as well. But only if easy. I'd like to make sure we can develop with the possibility of iterating later. This first phase is just for testing and proof of concept. The CLI tool is something that I would like to persist alongside the development of the visual web application.

### phase 2

If not added in the first step, we can expand image formats to raw handling, TIFF, AVIF, WebP, PNG, etc. This may also be a good time to start the GUI representation of the CLI. leaving the effects in place and not adding more yet, just to ensure the tool is functional.

### Phase 3

Perhaps this is when we can add options, such as strength meters that exaggerates the effect with a slider. Perhaps then we can add pixel sorting and masking to mask and generate effects on certain parts of the image. And this can be a simple mask, either hand-drawn, shape, vector, etc.

## architecture

I'm unsure what would be the best framework and stack here. I want it to be professional. The last GUI I tried to make used some Python library application window that looked decrepit. I've used glitch art and pixel sorting CLI tools using Python. But I'm curious if you can weigh the benefits of whether or not something like Rust would be helpful with this, whether there's other frameworks that I don't even know about that need to handle this, what kind of libraries we need, dependencies, etc. I.e. this probably goes deeper than using image magic for certain aspects of this?

## harness

I'll leave it up to you to suggest proper harnesses for Claude. Ensuring it checks and tests the work as it runs. Tell it to use best practice with Git and version control, utilizing branches for new features and phases. Being sure to commit when necessary, as frequently as it needs, but letting me, Phil, review and test before any pushing to main. This will be a raw repo. The bare bones of a project.

This will just be a very brief brief for a larger model to write the full spec, before actually writing code.