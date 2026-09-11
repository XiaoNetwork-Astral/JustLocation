package me.idk.justlocation.bridge;

import java.lang.reflect.Array;
import java.lang.reflect.Field;
import java.lang.reflect.InvocationHandler;
import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;
import java.lang.reflect.Proxy;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Set;

/** Per-listener substitution; periodic delivery uses the registry's current permission checks. */
final class TelephonyRegistryAdapter {
    interface Source { Object value(String packageName,int subId,int slot,String callback,Object authorized) throws Exception; }
    private record Event(int id,Method callback,boolean location) {}
    private final Class<?> listenerType;
    private final Field records,callback,packageName,subscription,phoneId,cellInfo,identity,signal,serviceState;
    private final Method validate,coarse,fine,match,resolvePhone,gsmSignal,sanitizeState;
    private final List<Event> events=new ArrayList<>();
    private final Set<Object> registries=new HashSet<>();
    private final Source source;

    TelephonyRegistryAdapter(Class<?> registry,Class<?> record,Class<?> listener,Class<?> constants,Source source) throws Exception {
        this.source=source;listenerType=listener;
        records=field(registry,"mRecords");callback=field(record,"callback");
        packageName=field(record,"callingPackage");subscription=field(record,"subId");phoneId=field(record,"phoneId");
        cellInfo=field(registry,"mCellInfo");identity=field(registry,"mCellIdentity");signal=field(registry,"mSignalStrength");
        serviceState=field(registry,"mServiceState");
        sanitizeState=serviceState.getType().getComponentType().getMethod("createLocationInfoSanitizedCopy",boolean.class);
        validate=method(registry,"validateEventAndUserLocked",record,int.class);
        coarse=method(registry,"checkCoarseLocationAccess",record,int.class);
        fine=method(registry,"checkFineLocationAccess",record,int.class);
        match=method(record,"matchTelephonyCallbackEvent",int.class);
        resolvePhone=method(registry,"getPhoneIdFromSubId",int.class);
        gsmSignal=signal.getType().getComponentType().getMethod("getGsmSignalStrength");
        addEvent(constants,"EVENT_CELL_INFO_CHANGED","onCellInfoChanged",true);
        addEvent(constants,"EVENT_CELL_LOCATION_CHANGED","onCellLocationChanged",true);
        addEvent(constants,"EVENT_SIGNAL_STRENGTHS_CHANGED","onSignalStrengthsChanged",false);
        addEvent(constants,"EVENT_SIGNAL_STRENGTH_CHANGED","onSignalStrengthChanged",false);
        addEvent(constants,"EVENT_SERVICE_STATE_CHANGED","onServiceStateChanged",false);
    }

    Object wrap(Object registry,String pkg,int subId,Object listener) {
        return Proxy.newProxyInstance(listenerType.getClassLoader(),new Class<?>[]{listenerType},new Listener(registry,pkg,subId,listener));
    }
    synchronized void track(Object registry) {registries.add(registry);}

    void dispatch() throws Exception {
        List<Object> current;
        synchronized(this) {current=List.copyOf(registries);}
        for(Object registry:current) {
            List<?> registrations=(List<?>)records.get(registry);
            synchronized(registrations) {
                for(Object record:registrations) {
                    Object target=callback.get(record);
                    if(target==null || !Proxy.isProxyClass(target.getClass()))continue;
                    InvocationHandler handler=Proxy.getInvocationHandler(target);
                    if(!(handler instanceof Listener listener) || listener.owner()!=this)continue;
                    for(Event event:events) {
                        if(!allowed(registry,record,event))continue;
                        listener.update(record);
                        Object authorized=event.callback.getName().equals("onServiceStateChanged")
                                ? authorizedState(registry,record,listener.slot) : null;
                        if(event.callback.getName().equals("onServiceStateChanged") && authorized==null)continue;
                        Object replacement=source.value(listener.pkg,listener.subId,listener.slot,event.callback.getName(),authorized);
                        if(replacement==null && !listener.synthetic.contains(event.id))continue;
                        Object value=replacement!=null ? replacement : authorized!=null ? authorized : originalValue(registry,listener.slot,event);
                        // Preserve null cell identities (the framework translates them to empty locations).
                        if(value==null && !event.callback.getName().equals("onCellLocationChanged"))continue;
                        try {
                            event.callback.invoke(listener.delegate,value);
                            if(replacement==null)listener.synthetic.remove(event.id);else listener.synthetic.add(event.id);
                        } catch(InvocationTargetException error) {
                            // Binder death is cleaned up by TelephonyRegistry's existing death recipient.
                            if(!isRemoteException(error.getCause()))throw error;
                        }
                    }
                }
            }
        }
    }

    private boolean allowed(Object registry,Object record,Event event) throws Exception {
        if(!event.location)return (boolean)match.invoke(record,event.id);
        return (boolean)validate.invoke(registry,record,event.id)
                && (boolean)coarse.invoke(registry,record,1) && (boolean)fine.invoke(registry,record,29);
    }

    private Object authorizedState(Object registry,Object record,int slot) throws Exception {
        Object state=slot<0?null:arrayValue(serviceState.get(registry),slot);
        if(state==null)return null;
        if((boolean)fine.invoke(registry,record,29))return state;
        return sanitizeState.invoke(state,!(boolean)coarse.invoke(registry,record,29));
    }

    private Object originalValue(Object registry,int slot,Event event) throws Exception {
        if(slot<0)return null;
        return switch(event.callback.getName()) {
            case "onCellInfoChanged" -> {List<?> values=(List<?>)cellInfo.get(registry);yield slot<values.size()?values.get(slot):null;}
            case "onCellLocationChanged" -> arrayValue(identity.get(registry),slot);
            case "onSignalStrengthsChanged" -> arrayValue(signal.get(registry),slot);
            case "onSignalStrengthChanged" -> {
                Object value=arrayValue(signal.get(registry),slot);
                int strength=value==null?99:(int)gsmSignal.invoke(value);yield strength==99?-1:strength;
            }
            default -> throw new IllegalStateException(event.callback.getName());
        };
    }

    private final class Listener implements InvocationHandler {
        final Object registry,delegate;final String pkg;
        int subId,slot=-1;final Set<Integer> synthetic=new HashSet<>();
        Listener(Object registry,String pkg,int subId,Object delegate) {this.registry=registry;this.pkg=pkg;this.subId=subId;this.delegate=delegate;}
        TelephonyRegistryAdapter owner() {return TelephonyRegistryAdapter.this;}
        void update(Object record) throws IllegalAccessException {subId=subscription.getInt(record);slot=phoneId.getInt(record);}
        public Object invoke(Object proxy,Method called,Object[] args) throws Throwable {
            Event event=null;
            for(Event item:events)if(item.callback.getName().equals(called.getName())){event=item;break;}
            if(event==null)return forward(called,delegate,args);
            List<?> registrations=(List<?>)records.get(registry);
            synchronized(registrations) {
                slot=(int)resolvePhone.invoke(registry,subId);
                for(Object record:registrations)if(callback.get(record)==proxy){update(record);break;}
                Object replacement=source.value(pkg,subId,slot,called.getName(),
                        called.getName().equals("onServiceStateChanged")?args[0]:null);
                Object result=forward(called,delegate,replacement==null?args:new Object[]{replacement});
                if(replacement==null)synthetic.remove(event.id);else synthetic.add(event.id);
                return result;
            }
        }
    }

    private void addEvent(Class<?> constants,String name,String callbackName,boolean location) throws Exception {
        int id=constants.getField(name).getInt(null);
        for(Method method:listenerType.getMethods())if(method.getName().equals(callbackName)) {
            events.add(new Event(id,method,location));return;
        }
        throw new NoSuchMethodException(callbackName);
    }
    private static boolean isRemoteException(Throwable error) {
        for(Class<?> type=error.getClass();type!=null;type=type.getSuperclass())if(type.getName().equals("android.os.RemoteException"))return true;
        return false;
    }
    private static Object arrayValue(Object array,int index) {return index<Array.getLength(array)?Array.get(array,index):null;}
    private static Object forward(Method method,Object target,Object[] args) throws Throwable {
        try{return method.invoke(target,args);}catch(InvocationTargetException error){throw error.getCause();}
    }
    private static Field field(Class<?> type,String name) throws Exception {Field field=type.getDeclaredField(name);field.setAccessible(true);return field;}
    private static Method method(Class<?> type,String name,Class<?>... args) throws Exception {Method method=type.getDeclaredMethod(name,args);method.setAccessible(true);return method;}
}
