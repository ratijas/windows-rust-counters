#ifndef _OFFSETS_H
#define _OFFSETS_H
//
//  symbols.h
//
//  Offset definition file for extensible counter objects and counters
//
//  These "relative" offsets must start at 0 and be multiples of 2 (i.e.
//  even numbers). In the Open Procedure, they will be added to the 
//  "First Counter" and "First Help" values of the device they belong to,
//  in order to determine the  absolute location of the counter and 
//  object names and corresponding help text in the registry.
//
//  this file is used by the extensible counter DLL code as well as the 
//  counter name and help text definition file (.INI) file that is used
//  by LODCTR to load the names into the registry.

#define MORSE_OBJECT    0

#define CHANNEL_SOS     2
#define CHANNEL_MOTD    4
#define CHANNEL_CUSTOM  6

#define LAST_MORSE_OBJECT_COUNTER_OFFSET  CHANNEL_CUSTOM

#endif
