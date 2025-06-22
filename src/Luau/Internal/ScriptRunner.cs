using System.Collections.Concurrent;
using System.Threading.Tasks.Sources;
using Luau.Native;
using static Luau.Native.NativeMethods;

namespace Luau;

internal sealed class ScriptRunner : IValueTaskSource<int>, IDisposable
{
    static readonly ConcurrentStack<ScriptRunner> pool = new();

    ManualResetValueTaskSourceCore<int> taskSource;
    bool isAsync;
    CancellationToken cancellationToken;

    public bool IsYieldFromInterrupt { get; set; }
    public bool IsAsync => isAsync;
    public CancellationToken CancellationToken => cancellationToken;

    ScriptRunner()
    {
    }

    public static ScriptRunner Rent()
    {
        if (!pool.TryPop(out var runner))
        {
            runner = new();
        }

        return runner;
    }

    public static void Return(ScriptRunner runner)
    {
        runner.cancellationToken = default;
        runner.IsYieldFromInterrupt = false;
        pool.Push(runner);
    }

    public int Run(LuauState state, int argCount, Span<LuauValue> destination)
    {
        try
        {
            var nResults = RunCore(state, argCount);

            if (destination.Length < nResults)
            {
                throw new ArgumentException("Destination is too short");
            }

            for (int i = nResults - 1; i >= 0; i--)
            {
                destination[i] = state.ToValue(-i);
            }

            return nResults;
        }
        finally
        {
            state.Runner = null;
        }
    }

    public LuauValue[] Run(LuauState state, int argCount)
    {
        try
        {
            var nResults = RunCore(state, argCount);
            if (nResults == 0) return [];

            var results = new LuauValue[nResults];
            for (int i = nResults - 1; i >= 0; i--)
            {
                results[i] = state.ToValue(-i);
            }

            return results;
        }
        finally
        {
            state.Runner = null;
        }
    }

    int RunCore(LuauState state, int argCount)
    {
        SetRunner(state);

        var prevStackIndex = state.GetAbsIndex(state.GetTop()) - argCount;
        isAsync = false;
        int status;

    STATUS_CHECK:
        IsYieldFromInterrupt = false;

        unsafe
        {
            status = lua_resume(state.AsPointer(), state.From == null ? null : state.From.AsPointer(), argCount);
        }

        switch ((lua_Status)status)
        {
            case lua_Status.LUA_OK:
                break;
            case lua_Status.LUA_YIELD:
                if (!IsYieldFromInterrupt) break;

                try
                {
                    // Since this is always a synchronous operation, await is not required (it is wrapped in ValueTask for error handling).
                    argCount = new ValueTask<int>(this, taskSource.Version).Result;
                }
                finally
                {
                    taskSource.Reset();
                }
                goto STATUS_CHECK;
            default:
                throw new LuauException(state.Pop().ToString());
        }

        unsafe
        {
            argCount = state.GetAbsIndex(state.GetTop()) - prevStackIndex + 1;
        }

        return argCount;
    }

    public async ValueTask<int> RunAsync(LuauState state, int argCount, Memory<LuauValue> destination, CancellationToken cancellationToken)
    {
        try
        {
            var nResults = await RunAsyncCore(state, argCount, cancellationToken);

            if (destination.Length < nResults)
            {
                throw new ArgumentException("Destination is too short");
            }

            for (int i = nResults - 1; i >= 0; i--)
            {
                destination.Span[i] = state.ToValue(-1);
            }

            return nResults;
        }
        finally
        {
            state.Runner = null;
        }
    }


    public async ValueTask<LuauValue[]> RunAsync(LuauState state, int argCount, CancellationToken cancellationToken)
    {
        try
        {
            var nResults = await RunAsyncCore(state, argCount, cancellationToken);
            if (nResults == 0) return [];

            var results = new LuauValue[nResults];
            for (int i = nResults - 1; i >= 0; i--)
            {
                results[i] = state.ToValue(-1);
            }

            return results;
        }
        finally
        {
            state.Runner = null;
        }
    }

    async ValueTask<int> RunAsyncCore(LuauState state, int argCount, CancellationToken cancellationToken)
    {
        SetRunner(state);

        var prevStackIndex = state.GetAbsIndex(state.GetTop()) - argCount;
        isAsync = true;
        this.cancellationToken = cancellationToken;

        int status;

    STATUS_CHECK:
        IsYieldFromInterrupt = false;

        unsafe
        {
            status = lua_resume(state.AsPointer(), state.From == null ? null : state.From.AsPointer(), argCount);
        }

        switch ((lua_Status)status)
        {
            case lua_Status.LUA_OK:
                break;
            case lua_Status.LUA_YIELD:
                if (!IsYieldFromInterrupt) break;

                CancellationTokenRegistration registration = default;
                if (cancellationToken.CanBeCanceled)
                {
#if NET8_0_OR_GREATER
                    registration = cancellationToken.UnsafeRegister(static (state, cancellationToken) =>
#else
                    registration = cancellationToken.UnsafeRegister((state) =>
#endif
                    {
                        var runner = (ScriptRunner)state!;
                        runner.TrySetException(new OperationCanceledException(cancellationToken));
                    }, this);
                }

                try
                {
                    argCount = await new ValueTask<int>(this, taskSource.Version);
                }
                finally
                {
                    taskSource.Reset();
                    registration.Dispose();
                }
                goto STATUS_CHECK;
            default:
                throw new LuauException(state.Pop().ToString());
        }

        unsafe
        {
            argCount = state.GetAbsIndex(state.GetTop()) - prevStackIndex + 1;
        }

        return argCount;
    }

    void SetRunner(LuauState state)
    {
        if (state.Runner != null)
        {
            ThrowHelper.ThrowInvalidOperationException("LuauState is running");
        }
        state.Runner = this;
    }

    public bool TrySetResult(int result)
    {
        if (taskSource.GetStatus(taskSource.Version) is ValueTaskSourceStatus.Pending)
        {
            taskSource.SetResult(result);
            return true;
        }

        return false;
    }

    public bool TrySetException(Exception exception)
    {
        if (taskSource.GetStatus(taskSource.Version) is ValueTaskSourceStatus.Pending)
        {
            taskSource.SetException(exception);
            return true;
        }

        return false;
    }

    int IValueTaskSource<int>.GetResult(short token)
    {
        return taskSource.GetResult(token);
    }

    ValueTaskSourceStatus IValueTaskSource<int>.GetStatus(short token)
    {
        return taskSource.GetStatus(token);
    }

    void IValueTaskSource<int>.OnCompleted(Action<object?> continuation, object? state, short token, ValueTaskSourceOnCompletedFlags flags)
    {
        taskSource.OnCompleted(continuation, state, token, flags);
    }

    public void Dispose()
    {
        Return(this);
    }
}