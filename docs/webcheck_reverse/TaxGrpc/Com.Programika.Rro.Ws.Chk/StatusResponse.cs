using System;
using System.Diagnostics;
using Google.Protobuf;
using Google.Protobuf.Reflection;

namespace Com.Programika.Rro.Ws.Chk;

public sealed class StatusResponse : IMessage<StatusResponse>, IMessage, IEquatable<StatusResponse>, IDeepCloneable<StatusResponse>
{
	[DebuggerNonUserCode]
	public static class Types
	{
		public enum Status
		{
			[OriginalName("UNKNOWN")]
			Unknown = 0,
			[OriginalName("OK")]
			Ok = 1,
			[OriginalName("ERROR_VEREFY")]
			ErrorVerefy = -1,
			[OriginalName("ERROR_CHECK")]
			ErrorCheck = -2,
			[OriginalName("ERROR_UNKNOWN")]
			ErrorUnknown = -4,
			[OriginalName("ERROR_NOT_REGISTERED_RRO")]
			ErrorNotRegisteredRro = -13,
			[OriginalName("ERROR_NOT_REGISTERED_SIGNER")]
			ErrorNotRegisteredSigner = -14
		}
	}

	private static readonly MessageParser<StatusResponse> _parser = new MessageParser<StatusResponse>(() => new StatusResponse());

	private UnknownFieldSet _unknownFields;

	public const int OpenShiftFieldNumber = 1;

	private bool openShift_;

	public const int OnlineFieldNumber = 2;

	private bool online_;

	public const int LastSignerFieldNumber = 3;

	private string lastSigner_ = "";

	public const int StatusFieldNumber = 4;

	private Types.Status status_;

	public const int ErrorMessageFieldNumber = 5;

	private string errorMessage_ = "";

	[DebuggerNonUserCode]
	public static MessageParser<StatusResponse> Parser => _parser;

	[DebuggerNonUserCode]
	public static MessageDescriptor Descriptor => GreetReflection.Descriptor.MessageTypes[4];

	[DebuggerNonUserCode]
	MessageDescriptor IMessage.Descriptor => Descriptor;

	[DebuggerNonUserCode]
	public bool OpenShift
	{
		get
		{
			return openShift_;
		}
		set
		{
			openShift_ = value;
		}
	}

	[DebuggerNonUserCode]
	public bool Online
	{
		get
		{
			return online_;
		}
		set
		{
			online_ = value;
		}
	}

	[DebuggerNonUserCode]
	public string LastSigner
	{
		get
		{
			return lastSigner_;
		}
		set
		{
			lastSigner_ = ProtoPreconditions.CheckNotNull(value, "value");
		}
	}

	[DebuggerNonUserCode]
	public Types.Status Status
	{
		get
		{
			return status_;
		}
		set
		{
			status_ = value;
		}
	}

	[DebuggerNonUserCode]
	public string ErrorMessage
	{
		get
		{
			return errorMessage_;
		}
		set
		{
			errorMessage_ = ProtoPreconditions.CheckNotNull(value, "value");
		}
	}

	[DebuggerNonUserCode]
	public StatusResponse()
	{
	}

	[DebuggerNonUserCode]
	public StatusResponse(StatusResponse other)
		: this()
	{
		openShift_ = other.openShift_;
		online_ = other.online_;
		lastSigner_ = other.lastSigner_;
		status_ = other.status_;
		errorMessage_ = other.errorMessage_;
		_unknownFields = UnknownFieldSet.Clone(other._unknownFields);
	}

	[DebuggerNonUserCode]
	public StatusResponse Clone()
	{
		return new StatusResponse(this);
	}

	[DebuggerNonUserCode]
	public override bool Equals(object other)
	{
		return Equals(other as StatusResponse);
	}

	[DebuggerNonUserCode]
	public bool Equals(StatusResponse other)
	{
		if (other == null)
		{
			return false;
		}
		if (other == this)
		{
			return true;
		}
		if (OpenShift != other.OpenShift)
		{
			return false;
		}
		if (Online != other.Online)
		{
			return false;
		}
		if (LastSigner != other.LastSigner)
		{
			return false;
		}
		if (Status != other.Status)
		{
			return false;
		}
		if (ErrorMessage != other.ErrorMessage)
		{
			return false;
		}
		return object.Equals(_unknownFields, other._unknownFields);
	}

	[DebuggerNonUserCode]
	public override int GetHashCode()
	{
		int num = 1;
		if (OpenShift)
		{
			num ^= OpenShift.GetHashCode();
		}
		if (Online)
		{
			num ^= Online.GetHashCode();
		}
		if (LastSigner.Length != 0)
		{
			num ^= LastSigner.GetHashCode();
		}
		if (Status != 0)
		{
			num ^= Status.GetHashCode();
		}
		if (ErrorMessage.Length != 0)
		{
			num ^= ErrorMessage.GetHashCode();
		}
		if (_unknownFields != null)
		{
			num ^= _unknownFields.GetHashCode();
		}
		return num;
	}

	[DebuggerNonUserCode]
	public override string ToString()
	{
		return JsonFormatter.ToDiagnosticString(this);
	}

	[DebuggerNonUserCode]
	public void WriteTo(CodedOutputStream output)
	{
		if (OpenShift)
		{
			output.WriteRawTag(8);
			output.WriteBool(OpenShift);
		}
		if (Online)
		{
			output.WriteRawTag(16);
			output.WriteBool(Online);
		}
		if (LastSigner.Length != 0)
		{
			output.WriteRawTag(26);
			output.WriteString(LastSigner);
		}
		if (Status != 0)
		{
			output.WriteRawTag(32);
			output.WriteEnum((int)Status);
		}
		if (ErrorMessage.Length != 0)
		{
			output.WriteRawTag(42);
			output.WriteString(ErrorMessage);
		}
		if (_unknownFields != null)
		{
			_unknownFields.WriteTo(output);
		}
	}

	[DebuggerNonUserCode]
	public int CalculateSize()
	{
		int num = 0;
		if (OpenShift)
		{
			num += 2;
		}
		if (Online)
		{
			num += 2;
		}
		if (LastSigner.Length != 0)
		{
			num += 1 + CodedOutputStream.ComputeStringSize(LastSigner);
		}
		if (Status != 0)
		{
			num += 1 + CodedOutputStream.ComputeEnumSize((int)Status);
		}
		if (ErrorMessage.Length != 0)
		{
			num += 1 + CodedOutputStream.ComputeStringSize(ErrorMessage);
		}
		if (_unknownFields != null)
		{
			num += _unknownFields.CalculateSize();
		}
		return num;
	}

	[DebuggerNonUserCode]
	public void MergeFrom(StatusResponse other)
	{
		if (other != null)
		{
			if (other.OpenShift)
			{
				OpenShift = other.OpenShift;
			}
			if (other.Online)
			{
				Online = other.Online;
			}
			if (other.LastSigner.Length != 0)
			{
				LastSigner = other.LastSigner;
			}
			if (other.Status != 0)
			{
				Status = other.Status;
			}
			if (other.ErrorMessage.Length != 0)
			{
				ErrorMessage = other.ErrorMessage;
			}
			_unknownFields = UnknownFieldSet.MergeFrom(_unknownFields, other._unknownFields);
		}
	}

	[DebuggerNonUserCode]
	public void MergeFrom(CodedInputStream input)
	{
		uint num;
		while ((num = input.ReadTag()) != 0)
		{
			switch (num)
			{
			default:
				_unknownFields = UnknownFieldSet.MergeFieldFrom(_unknownFields, input);
				break;
			case 8u:
				OpenShift = input.ReadBool();
				break;
			case 16u:
				Online = input.ReadBool();
				break;
			case 26u:
				LastSigner = input.ReadString();
				break;
			case 32u:
				Status = (Types.Status)input.ReadEnum();
				break;
			case 42u:
				ErrorMessage = input.ReadString();
				break;
			}
		}
	}
}
