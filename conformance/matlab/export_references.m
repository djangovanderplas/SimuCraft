function export_references(outdir)
%EXPORT_REFERENCES Make the Simulink reference runs for SimuCraft (WP1.5).
%
%   export_references('../references') builds 9 small models from code,
%   simulates each model two times and writes, for each model:
%     <name>.slx          the model
%     <name>.ode4.csv     fixed-step RK4, step 1e-3 (compare with our RK4)
%     <name>.tight.csv    ode45, RelTol 1e-10, AbsTol 1e-12, output every 1e-3
%                         (a near-exact solution for any solver)
%     <name>.json         solver settings, block parameters, MATLAB version
%
%   The script builds the models from code, so SimuCraft can build the same
%   models. Do not edit the models by hand. A failed model does not stop the
%   other models. The script prints a summary at the end.

if nargin < 1
    outdir = fullfile(pwd, 'references');
end
if ~exist(outdir, 'dir')
    mkdir(outdir);
end
load_system('simulink');

models = {@basic_sources, @integrator_ramp, @mass_spring_damper, ...
    @transfer_fcn_step, @pid_plant, @saturation_sine, ...
    @saturating_integrator, @discrete_pid_1khz, @multirate};

runs = struct( ...
    'name', {'ode4', 'tight'}, ...
    'settings', { ...
        {'SolverType', 'Fixed-step', 'Solver', 'ode4', 'FixedStep', '1e-3'}, ...
        {'SolverType', 'Variable-step', 'Solver', 'ode45', 'RelTol', '1e-10', ...
         'AbsTol', '1e-12', 'MaxStep', '1e-2'}});

failed = {};
for i = 1:numel(models)
    name = func2str(models{i});
    try
        if bdIsLoaded(name)
            close_system(name, 0);
        end
        new_system(name);
        stop_time = models{i}(name);
        set_param(name, 'StopTime', num2str(stop_time), 'SaveTime', 'on', ...
            'SaveOutput', 'on', 'SaveFormat', 'Array', 'SaveState', 'off', ...
            'SignalLogging', 'off', 'ReturnWorkspaceOutputs', 'on');
        try
            Simulink.BlockDiagram.arrangeSystem(name);
        catch
            % Layout only. Older releases do not have arrangeSystem.
        end
        outputs = outport_names(name);
        run_meta = {};
        for r = runs
            set_param(name, r.settings{:});
            if strcmp(r.name, 'tight')
                set_param(name, 'OutputOption', 'SpecifiedOutputTimes', ...
                    'OutputTimes', sprintf('0:1e-3:%g', stop_time));
            end
            out = sim(name);
            write_csv(fullfile(outdir, sprintf('%s.%s.csv', name, r.name)), ...
                outputs, out.get('tout'), out.get('yout'));
            run_meta{end + 1} = struct('name', r.name, 'settings', ...
                struct(r.settings{:})); %#ok<AGROW>
        end
        save_system(name, fullfile(outdir, [name '.slx']));
        sl = ver('simulink');
        meta = struct('model', name, 'stop_time', stop_time, ...
            'matlab', version, 'simulink', sl.Version, ...
            'created', char(datetime('now', 'Format', 'yyyy-MM-dd''T''HH:mm:ss')), ...
            'outputs', {outputs}, 'runs', {run_meta}, 'blocks', {block_list(name)});
        write_json(fullfile(outdir, [name '.json']), meta);
        close_system(name, 0);
        fprintf('ok      %s\n', name);
    catch err
        fprintf(2, 'FAILED  %s: %s\n', name, err.message);
        failed{end + 1} = name; %#ok<AGROW>
        if bdIsLoaded(name)
            close_system(name, 0);
        end
    end
end
fprintf('\n%d of %d models exported to %s\n', ...
    numel(models) - numel(failed), numel(models), outdir);
if ~isempty(failed)
    fprintf(2, 'Failed: %s\n', strjoin(failed, ', '));
end
end

% ------------------------------------------------------------------ models
% Each function builds one model and returns its stop time.

function T = basic_sources(m)
% Constant, Step, Sum and Gain: y = 0.5 * (2 - step(t >= 1, 0 -> 3)).
add(m, 'simulink/Sources/Constant', 'c', {'Value', '2'});
add(m, 'simulink/Sources/Step', 'step', {'Time', '1', 'Before', '0', 'After', '3', 'SampleTime', '0'});
add(m, 'simulink/Math Operations/Sum', 'sum', {'Inputs', '+-'});
add(m, 'simulink/Math Operations/Gain', 'gain', {'Gain', '0.5'});
out(m, 'y', 1);
wire(m, 'c/1', 'sum/1'); wire(m, 'step/1', 'sum/2');
wire(m, 'sum/1', 'gain/1'); wire(m, 'gain/1', 'y/1');
T = 3;
end

function T = integrator_ramp(m)
% x' = 1, x(0) = 0.5.
add(m, 'simulink/Sources/Constant', 'c', {'Value', '1'});
add(m, 'simulink/Continuous/Integrator', 'int', {'InitialCondition', '0.5'});
out(m, 'x', 1);
wire(m, 'c/1', 'int/1'); wire(m, 'int/1', 'x/1');
T = 5;
end

function T = mass_spring_damper(m)
% x'' = -0.5 x' - 4 x, x(0) = 1. Same model as the SimuCraft IR fixture.
add(m, 'simulink/Continuous/Integrator', 'v_int', {'InitialCondition', '0'});
add(m, 'simulink/Continuous/Integrator', 'x_int', {'InitialCondition', '1'});
add(m, 'simulink/Math Operations/Gain', 'c', {'Gain', '-0.5'});
add(m, 'simulink/Math Operations/Gain', 'k', {'Gain', '-4'});
add(m, 'simulink/Math Operations/Sum', 'a', {'Inputs', '++'});
out(m, 'x', 1); out(m, 'v', 2);
wire(m, 'v_int/1', 'x_int/1'); wire(m, 'v_int/1', 'c/1'); wire(m, 'x_int/1', 'k/1');
wire(m, 'c/1', 'a/1'); wire(m, 'k/1', 'a/2'); wire(m, 'a/1', 'v_int/1');
wire(m, 'x_int/1', 'x/1'); wire(m, 'v_int/1', 'v/1');
T = 10;
end

function T = transfer_fcn_step(m)
% Unit step into 4 / (s^2 + 0.5 s + 4).
add(m, 'simulink/Sources/Step', 'step', {'Time', '0', 'Before', '0', 'After', '1', 'SampleTime', '0'});
add(m, 'simulink/Continuous/Transfer Fcn', 'tf', {'Numerator', '[4]', 'Denominator', '[1 0.5 4]'});
out(m, 'y', 1);
wire(m, 'step/1', 'tf/1'); wire(m, 'tf/1', 'y/1');
T = 10;
end

function T = pid_plant(m)
% Gate G1 model: continuous PID (parallel form) around 2 / (0.5 s + 1).
add(m, 'simulink/Sources/Step', 'ref', {'Time', '0.1', 'Before', '0', 'After', '1', 'SampleTime', '0'});
add(m, 'simulink/Math Operations/Sum', 'err', {'Inputs', '+-'});
add(m, 'simulink/Continuous/PID Controller', 'pid', ...
    {'Controller', 'PID', 'P', '2', 'I', '5', 'D', '0.1', 'N', '100'});
add(m, 'simulink/Continuous/Transfer Fcn', 'plant', {'Numerator', '[2]', 'Denominator', '[0.5 1]'});
out(m, 'y', 1); out(m, 'u', 2);
wire(m, 'ref/1', 'err/1'); wire(m, 'err/1', 'pid/1'); wire(m, 'pid/1', 'plant/1');
wire(m, 'plant/1', 'err/2'); wire(m, 'plant/1', 'y/1'); wire(m, 'pid/1', 'u/1');
T = 5;
end

function T = saturation_sine(m)
% A 0.5 Hz sine of amplitude 2 through a saturation at +-1 (zero crossings).
add(m, 'simulink/Sources/Sine Wave', 'sine', {'Amplitude', '2', 'Frequency', 'pi', 'SampleTime', '0'});
add(m, 'simulink/Discontinuities/Saturation', 'sat', {'UpperLimit', '1', 'LowerLimit', '-1'});
out(m, 'y', 1); out(m, 'u', 2);
wire(m, 'sine/1', 'sat/1'); wire(m, 'sat/1', 'y/1'); wire(m, 'sine/1', 'u/1');
T = 4;
end

function T = saturating_integrator(m)
% x' = sin(t), x limited to [-0.5, 0.5] (requirement E3).
add(m, 'simulink/Sources/Sine Wave', 'sine', {'Amplitude', '1', 'Frequency', '1', 'SampleTime', '0'});
add(m, 'simulink/Continuous/Integrator', 'int', {'InitialCondition', '0', ...
    'LimitOutput', 'on', 'UpperSaturationLimit', '0.5', 'LowerSaturationLimit', '-0.5'});
out(m, 'x', 1);
wire(m, 'sine/1', 'int/1'); wire(m, 'int/1', 'x/1');
T = 10;
end

function T = discrete_pid_1khz(m)
% Discrete PI at 1 kHz (forward Euler) around a continuous plant.
add(m, 'simulink/Sources/Step', 'ref', {'Time', '0.1', 'Before', '0', 'After', '1', 'SampleTime', '0'});
add(m, 'simulink/Math Operations/Sum', 'err', {'Inputs', '+-'});
add(m, 'simulink/Discrete/Discrete PID Controller', 'pi', ...
    {'Controller', 'PI', 'P', '2', 'I', '5', 'SampleTime', '0.001'});
add(m, 'simulink/Continuous/Transfer Fcn', 'plant', {'Numerator', '[2]', 'Denominator', '[0.5 1]'});
out(m, 'y', 1); out(m, 'u', 2);
wire(m, 'ref/1', 'err/1'); wire(m, 'err/1', 'pi/1'); wire(m, 'pi/1', 'plant/1');
wire(m, 'plant/1', 'err/2'); wire(m, 'plant/1', 'y/1'); wire(m, 'pi/1', 'u/1');
T = 3;
end

function T = multirate(m)
% 1 kHz and 100 Hz parts with a rate transition (requirement M7).
add(m, 'simulink/Sources/Sine Wave', 'sine', {'Amplitude', '1', 'Frequency', '2*pi', 'SampleTime', '0'});
add(m, 'simulink/Discrete/Zero-Order Hold', 'zoh_fast', {'SampleTime', '0.001'});
add(m, 'simulink/Math Operations/Gain', 'gain', {'Gain', '2'});
add(m, 'simulink/Signal Attributes/Rate Transition', 'rt', {'OutPortSampleTime', '0.01'});
add(m, 'simulink/Discrete/Unit Delay', 'delay', {'InitialCondition', '0', 'SampleTime', '0.01'});
out(m, 'fast', 1); out(m, 'slow', 2);
wire(m, 'sine/1', 'zoh_fast/1'); wire(m, 'zoh_fast/1', 'gain/1');
wire(m, 'gain/1', 'rt/1'); wire(m, 'rt/1', 'delay/1');
wire(m, 'gain/1', 'fast/1'); wire(m, 'delay/1', 'slow/1');
T = 1;
end

% ----------------------------------------------------------------- helpers

function add(m, lib, name, params)
add_block(lib, [m '/' name], params{:});
end

function out(m, name, port)
add_block('simulink/Sinks/Out1', [m '/' name], 'Port', num2str(port));
end

function wire(m, from, to)
add_line(m, from, to, 'autorouting', 'on');
end

function names = outport_names(m)
blocks = find_system(m, 'SearchDepth', 1, 'BlockType', 'Outport');
ports = str2double(get_param(blocks, 'Port'));
[~, order] = sort(ports);
names = get_param(blocks(order), 'Name')';
end

function write_csv(file, names, t, y)
% %.17g keeps every bit of a double.
fid = fopen(file, 'w');
fprintf(fid, '%s\n', strjoin([{'time'}, names], ','));
fprintf(fid, [repmat('%.17g,', 1, numel(names)) '%.17g\n'], [t y]');
fclose(fid);
end

function blocks = block_list(m)
paths = find_system(m, 'SearchDepth', 1, 'Type', 'block');
blocks = cell(1, numel(paths));
for k = 1:numel(paths)
    p = paths{k};
    params = struct();
    dialog = get_param(p, 'DialogParameters');
    if isstruct(dialog)
        for f = fieldnames(dialog)'
            try
                v = get_param(p, f{1});
                if ischar(v)
                    params.(f{1}) = v;
                end
            catch
                % Some dialog parameters cannot be read outside the dialog.
            end
        end
    end
    blocks{k} = struct('name', get_param(p, 'Name'), 'type', get_param(p, 'BlockType'), ...
        'mask', get_param(p, 'MaskType'), 'params', params);
end
end

function write_json(file, value)
try
    text = jsonencode(value, 'PrettyPrint', true);
catch
    text = jsonencode(value); % Releases before R2021a.
end
fid = fopen(file, 'w');
fprintf(fid, '%s\n', text);
fclose(fid);
end
