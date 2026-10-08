// ACEmulator/ACE 47edade3bd3f6044b676d4eb877c4965c7eda62b; AGPL-3.0-only. Unchanged status methods.
// Source Source/ACE.Server/WorldObjects/WorldObject_Generators.cs SHA256 54d8b951e9714730ecd2e2dddcfc933e7a0a05485d4b06fc858db12d0f27bff3
using System;
public partial class WorldObject {private bool eventStatusChanged;
        public void CheckGeneratorStatus()
        {
            switch (GeneratorTimeType)
            {
                // TODO: defined
                case GeneratorTimeType.RealTime:
                    CheckRealTimeStatus();
                    break;
                case GeneratorTimeType.Event:
                    CheckEventStatus();
                    break;
                case GeneratorTimeType.Night:
                case GeneratorTimeType.Day:
                    CheckTimeOfDayStatus();
                    break;
            }            
        }
        public void CheckTimeOfDayStatus()
        {
            var prevDisabled = GeneratorDisabled;
           
            var isDay = Timers.CurrentInGameTime.IsDay;
            var isDayGenerator = GeneratorTimeType == GeneratorTimeType.Day;

            //GeneratorDisabled = isDay != isDayGenerator;
            //HandleStatus(prevDisabled);

            HandleStatusStaged(prevDisabled, isDay, isDayGenerator);
        }
        public void CheckRealTimeStatus()
        {
            var prevDisabled = GeneratorDisabled;

            var now = (int)Time.GetUnixTime();

            var start = (now < GeneratorStartTime) && (GeneratorStartTime > 0);
            var end = (now > GeneratorEndTime) && (GeneratorEndTime > 0);

            //GeneratorDisabled = ((now < GeneratorStartTime) && (GeneratorStartTime > 0)) || ((now > GeneratorEndTime) && (GeneratorEndTime > 0));
            //HandleStatus(prevDisabled);

            HandleStatusStaged(prevDisabled, start, end);
        }
        public void CheckEventStatus()
        {
            if (string.IsNullOrEmpty(GeneratorEvent))
                return;

            var prevState = GeneratorDisabled;

            if (!EventManager.IsEventAvailable(GeneratorEvent))
                return;

            var enabled = EventManager.IsEventEnabled(GeneratorEvent);
            var started = EventManager.IsEventStarted(GeneratorEvent, this, null);

            //GeneratorDisabled = !enabled || !started;
            //HandleStatus(prevState);

            HandleStatusStaged(prevState, enabled, started);
        }
        public void HandleStatusStaged(bool prevDisabled, bool cond1, bool cond2)
        {
            var change = false;
            switch (GeneratorTimeType)
            {
                case GeneratorTimeType.RealTime:
                    change = cond1 || cond2;
                    break;
                case GeneratorTimeType.Event:
                    change = !cond1 || !cond2;
                    break;
                case GeneratorTimeType.Day:
                case GeneratorTimeType.Night:
                    change = cond1 != cond2;
                    break;
            }

            if (eventStatusChanged)
            {
                GeneratorDisabled = change;

                if (!GeneratorDisabled)
                    StartGenerator();
                else
                    DisableGenerator();

                eventStatusChanged = false;
            }
            else
            {
                if (prevDisabled != change)
                    eventStatusChanged = true;
            }
        }
        public void Generator_Update()
        {
            //Console.WriteLine($"{Name}.Generator_HeartBeat({HeartbeatInterval})");

            if (!FirstEnterWorldDone)
                FirstEnterWorldDone = true;

            CheckGeneratorStatus();

            if (!GeneratorEnteredWorld)
            {
                CheckGeneratorStatus(); // due to staging if generator hadn't entered world, reprocess CheckGeneratorStatus for first generator status to update

                if (!GeneratorDisabled)
                    StartGenerator();   // spawn initial objects for this generator

                GeneratorEnteredWorld = true;
            }
        }
}
