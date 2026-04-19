using System;
using System.Diagnostics;
using Google.Protobuf;
using Google.Protobuf.Reflection;

namespace Com.Programika.Rro.Ws.Chk;

public sealed class CheckResponse : IMessage<CheckResponse>, IMessage, IEquatable<CheckResponse>, IDeepCloneable<CheckResponse>
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
			[OriginalName("ERROR_SAVE")]
			ErrorSave = -3,
			[OriginalName("ERROR_UNKNOWN")]
			ErrorUnknown = -4,
			[OriginalName("ERROR_TYPE")]
			ErrorType = -5,
			[OriginalName("ERROR_NOT_PREV_ZREPORT")]
			ErrorNotPrevZreport = -6,
			[OriginalName("ERROR_XML")]
			ErrorXml = -7,
			[OriginalName("ERROR_XML_DATE")]
			ErrorXmlDate = -8,
			[OriginalName("ERROR_XML_CHK")]
			ErrorXmlChk = -9,
			[OriginalName("ERROR_XML_ZREPORT")]
			ErrorXmlZreport = -10,
			[OriginalName("ERROR_OFFLINE_168")]
			ErrorOffline168 = -11,
			[OriginalName("ERROR_BAD_HASH_PREV")]
			ErrorBadHashPrev = -12,
			[OriginalName("ERROR_NOT_REGISTERED_RRO")]
			ErrorNotRegisteredRro = -13,
			[OriginalName("ERROR_NOT_REGISTERED_SIGNER")]
			ErrorNotRegisteredSigner = -14,
			[OriginalName("ERROR_NOT_OPEN_SHIFT")]
			ErrorNotOpenShift = -15,
			[OriginalName("ERROR_OFFLINE_ID")]
			ErrorOfflineId = -16
		}
	}

	private static readonly MessageParser<CheckResponse> _parser = new MessageParser<CheckResponse>(() => new CheckResponse());

	private UnknownFieldSet _unknownFields;

	public const int IdFieldNumber = 1;

	private string id_ = "";

	public const int StatusFieldNumber = 2;

	private Types.Status status_;

	public const int IdSignFieldNumber = 3;

	private ByteString idSign_ = ByteString.Empty;

	public const int DataSignFieldNumber = 4;

	private ByteString dataSign_ = ByteString.Empty;

	public const int ErrorMessageFieldNumber = 5;

	private string errorMessage_ = "";

	[DebuggerNonUserCode]
	public static MessageParser<CheckResponse> Parser => _parser;

	[DebuggerNonUserCode]
	public static MessageDescriptor Descriptor => GreetReflection.Descriptor.MessageTypes[3];

	[DebuggerNonUserCode]
	MessageDescriptor IMessage.Descriptor => Descriptor;

	[DebuggerNonUserCode]
	public string Id
	{
		get
		{
			return id_;
		}
		set
		{
			id_ = ProtoPreconditions.CheckNotNull(value, "value");
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
	public ByteString IdSign
	{
		get
		{
			return idSign_;
		}
		set
		{
			idSign_ = ProtoPreconditions.CheckNotNull(value, "value");
		}
	}

	[DebuggerNonUserCode]
	public ByteString DataSign
	{
		get
		{
			return dataSign_;
		}
		set
		{
			dataSign_ = ProtoPreconditions.CheckNotNull(value, "value");
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
	public CheckResponse()
	{
	}

	[DebuggerNonUserCode]
	public CheckResponse(CheckResponse other)
		: this()
	{
		id_ = other.id_;
		status_ = other.status_;
		idSign_ = other.idSign_;
		dataSign_ = other.dataSign_;
		errorMessage_ = other.errorMessage_;
		_unknownFields = UnknownFieldSet.Clone(other._unknownFields);
	}

	[DebuggerNonUserCode]
	public CheckResponse Clone()
	{
		return new CheckResponse(this);
	}

	[DebuggerNonUserCode]
	public override bool Equals(object other)
	{
		return Equals(other as CheckResponse);
	}

	[DebuggerNonUserCode]
	public bool Equals(CheckResponse other)
	{
		if (other == null)
		{
			return false;
		}
		if (other == this)
		{
			return true;
		}
		if (Id != other.Id)
		{
			return false;
		}
		if (Status != other.Status)
		{
			return false;
		}
		if (IdSign != other.IdSign)
		{
			return false;
		}
		if (DataSign != other.DataSign)
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
		if (Id.Length != 0)
		{
			num ^= Id.GetHashCode();
		}
		if (Status != 0)
		{
			num ^= Status.GetHashCode();
		}
		if (IdSign.Length != 0)
		{
			num ^= IdSign.GetHashCode();
		}
		if (DataSign.Length != 0)
		{
			num ^= DataSign.GetHashCode();
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
		if (Id.Length != 0)
		{
			output.WriteRawTag(10);
			output.WriteString(Id);
		}
		if (Status != 0)
		{
			output.WriteRawTag(16);
			output.WriteEnum((int)Status);
		}
		if (IdSign.Length != 0)
		{
			output.WriteRawTag(26);
			output.WriteBytes(IdSign);
		}
		if (DataSign.Length != 0)
		{
			output.WriteRawTag(34);
			output.WriteBytes(DataSign);
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
		if (Id.Length != 0)
		{
			num += 1 + CodedOutputStream.ComputeStringSize(Id);
		}
		if (Status != 0)
		{
			num += 1 + CodedOutputStream.ComputeEnumSize((int)Status);
		}
		if (IdSign.Length != 0)
		{
			num += 1 + CodedOutputStream.ComputeBytesSize(IdSign);
		}
		if (DataSign.Length != 0)
		{
			num += 1 + CodedOutputStream.ComputeBytesSize(DataSign);
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
	public void MergeFrom(CheckResponse other)
	{
		if (other != null)
		{
			if (other.Id.Length != 0)
			{
				Id = other.Id;
			}
			if (other.Status != 0)
			{
				Status = other.Status;
			}
			if (other.IdSign.Length != 0)
			{
				IdSign = other.IdSign;
			}
			if (other.DataSign.Length != 0)
			{
				DataSign = other.DataSign;
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
			case 10u:
				Id = input.ReadString();
				break;
			case 16u:
				Status = (Types.Status)input.ReadEnum();
				break;
			case 26u:
				IdSign = input.ReadBytes();
				break;
			case 34u:
				DataSign = input.ReadBytes();
				break;
			case 42u:
				ErrorMessage = input.ReadString();
				break;
			}
		}
	}
}
